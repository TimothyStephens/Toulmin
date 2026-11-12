//! Segments and batch framing (item 33; 11.3; 11.21): the header line, the
//! payload, plain and packed forms, sealing, and the frame walk that scans
//! and verification share.

use std::fmt::Write as _;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tm_formats::id::EventId;

use crate::{Code, Error, Result, fsync_dir};

/// 11.3: a segment is sealed before a batch that would carry it past this.
pub const SEGMENT_LIMIT: u64 = 64 << 20;

/// zstd level for sealed segments (11.21).
pub const SEAL_LEVEL: i32 = 9;

/// `seg-NNNNNN`.
#[must_use]
pub fn segment_name(n: u64) -> String {
    format!("seg-{n:06}")
}

/// `main/seg-NNNNNN.jsonl`.
#[must_use]
pub fn plain_path(main: &Path, n: u64) -> PathBuf {
    main.join(format!("{}.jsonl", segment_name(n)))
}

/// `main/seg-NNNNNN.jsonl.zst`.
#[must_use]
pub fn packed_path(main: &Path, n: u64) -> PathBuf {
    main.join(format!("{}.jsonl.zst", segment_name(n)))
}

/// The header line of a batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchHeader {
    /// Batch number, from 1.
    pub b: u64,
    /// Events in the batch.
    pub n: u64,
    /// Payload bytes, newlines included.
    pub len: u64,
    /// `sha256:` of the payload.
    pub h: EventId,
    /// The writer's epoch.
    pub epoch: u64,
}

impl BatchHeader {
    /// The header line, canonical JSON plus a newline.
    #[must_use]
    pub fn line(&self) -> String {
        let v: Value = json!({"b": self.b, "n": self.n, "len": self.len, "h": self.h.as_str(), "epoch": self.epoch});
        let mut s = tm_formats::canonical::canonical(&v).unwrap_or_default();
        s.push('\n');
        s
    }

    /// Parse a header line (without its newline); `None` for anything else.
    #[must_use]
    pub fn parse(line: &[u8]) -> Option<BatchHeader> {
        let v = tm_formats::canonical::parse_bytes(line).ok()?;
        let m = v.as_object()?;
        if m.len() != 5 {
            return None;
        }
        let int = |k: &str| m.get(k)?.as_u64();
        let header = BatchHeader {
            b: int("b")?,
            n: int("n")?,
            len: int("len")?,
            h: EventId::parse(m.get("h")?.as_str()?)?,
            epoch: int("epoch")?,
        };
        // One encoding: the line must be the canonical form.
        (header.line().as_bytes() == [line, b"\n"].concat()).then_some(header)
    }
}

/// `sha256:` of a payload.
#[must_use]
pub fn payload_hash(payload: &[u8]) -> EventId {
    id_from_digest(&Sha256::digest(payload))
}

/// `sha256:` plus the hex of a 32-byte digest.
pub(crate) fn id_from_digest(digest: &[u8]) -> EventId {
    let mut s = String::with_capacity(71);
    s.push_str("sha256:");
    for b in digest {
        let _ = write!(s, "{b:02x}");
    }
    EventId::parse(&s).unwrap_or_else(|| unreachable!("hex digest"))
}

/// Split a payload into its newline-terminated lines (newline excluded);
/// `None` when it does not end with a newline.
#[must_use]
pub fn split_lines(payload: &[u8]) -> Option<Vec<&[u8]>> {
    if payload.is_empty() {
        return Some(Vec::new());
    }
    if *payload.last()? != b'\n' {
        return None;
    }
    Some(
        payload[..payload.len() - 1]
            .split(|b| *b == b'\n')
            .collect(),
    )
}

/// Which file a segment was read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    /// `seg-NNNNNN.jsonl`.
    Plain,
    /// `seg-NNNNNN.jsonl.zst`.
    Packed,
}

/// A segment's uncompressed bytes.
#[derive(Debug, Clone)]
pub struct Loaded {
    /// The bytes, positions are offsets into them.
    pub bytes: Vec<u8>,
    /// Which form was read (plain preferred when both exist, 11.21).
    pub form: Form,
}

/// Why a segment could not be loaded.
#[derive(Debug)]
pub enum LoadError {
    /// The operating system refused.
    Io(PathBuf, std::io::Error),
    /// The packed file does not decode (a zstd error or checksum failure).
    PackedCorrupt(u64, String),
}

impl From<LoadError> for Error {
    fn from(e: LoadError) -> Self {
        match e {
            LoadError::Io(p, s) => Error::io(p, s),
            LoadError::PackedCorrupt(n, why) => Error::code(
                Code::E2103,
                format!("packed segment {} does not decode: {why}", segment_name(n)),
            ),
        }
    }
}

/// Which forms exist for segment `n`.
#[must_use]
pub fn forms(main: &Path, n: u64) -> (bool, bool) {
    (plain_path(main, n).exists(), packed_path(main, n).exists())
}

/// Load segment `n`: the plain file when present, else the packed one;
/// `None` when neither exists.
///
/// # Errors
/// `LoadError`.
pub fn load(main: &Path, n: u64) -> std::result::Result<Option<Loaded>, LoadError> {
    let plain = plain_path(main, n);
    match std::fs::read(&plain) {
        Ok(bytes) => {
            return Ok(Some(Loaded {
                bytes,
                form: Form::Plain,
            }));
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(LoadError::Io(plain, e)),
    }
    let packed = packed_path(main, n);
    match std::fs::read(&packed) {
        Ok(compressed) => match zstd::decode_all(&compressed[..]) {
            Ok(bytes) => Ok(Some(Loaded {
                bytes,
                form: Form::Packed,
            })),
            Err(e) => Err(LoadError::PackedCorrupt(n, e.to_string())),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(LoadError::Io(packed, e)),
    }
}

/// The length of the open segment's plain file, 0 when absent.
///
/// # Errors
/// `Error::Io` for anything but a missing file.
pub fn plain_len(main: &Path, n: u64) -> Result<u64> {
    let p = plain_path(main, n);
    match std::fs::metadata(&p) {
        Ok(md) => Ok(md.len()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(e) => Err(Error::io(p, e)),
    }
}

/// One framed batch inside a segment's bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame<'a> {
    /// The header.
    pub header: BatchHeader,
    /// Offset of the header line.
    pub header_offset: u64,
    /// Offset of the first payload byte.
    pub payload_offset: u64,
    /// The payload (`len` bytes).
    pub payload: &'a [u8],
    /// Offset just past the payload.
    pub end: u64,
}

/// Why a frame walk stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    /// No complete header line, or not a header, at `offset`.
    BadHeader {
        /// Where.
        offset: u64,
    },
    /// The header's `len` is not satisfied before `end`.
    Short {
        /// The header's offset.
        offset: u64,
        /// The header.
        header: BatchHeader,
        /// Bytes present after the header line.
        have: u64,
    },
}

/// Walk the frames in `bytes[start..end]`.
pub struct Frames<'a> {
    bytes: &'a [u8],
    pos: usize,
    end: usize,
    stopped: bool,
}

/// Frames from `start` to `end` (both offsets into `bytes`).
#[must_use]
pub fn frames(bytes: &[u8], start: u64, end: u64) -> Frames<'_> {
    let end = usize::try_from(end).unwrap_or(usize::MAX).min(bytes.len());
    Frames {
        bytes,
        pos: usize::try_from(start).unwrap_or(usize::MAX).min(end),
        end,
        stopped: false,
    }
}

impl<'a> Iterator for Frames<'a> {
    type Item = std::result::Result<Frame<'a>, FrameError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.stopped || self.pos >= self.end {
            return None;
        }
        let offset = self.pos as u64;
        let rest = &self.bytes[self.pos..self.end];
        let Some(nl) = rest.iter().position(|b| *b == b'\n') else {
            self.stopped = true;
            return Some(Err(FrameError::BadHeader { offset }));
        };
        let Some(header) = BatchHeader::parse(&rest[..nl]) else {
            self.stopped = true;
            return Some(Err(FrameError::BadHeader { offset }));
        };
        let payload_start = self.pos + nl + 1;
        let have = (self.end - payload_start) as u64;
        if have < header.len {
            self.stopped = true;
            return Some(Err(FrameError::Short {
                offset,
                header,
                have,
            }));
        }
        let len = usize::try_from(header.len).unwrap_or(usize::MAX);
        let payload = &self.bytes[payload_start..payload_start + len];
        self.pos = payload_start + len;
        Some(Ok(Frame {
            header,
            header_offset: offset,
            payload_offset: payload_start as u64,
            payload,
            end: self.pos as u64,
        }))
    }
}

/// Seal segment `n` (11.21): pack the plain file at level 9 with a content
/// checksum into a temporary file, `fsync`, re-read and compare with the
/// plain bytes, rename into place, `fsync` the directory, remove the plain
/// file, `fsync` the directory again.
///
/// # Errors
/// `Error::Io`; `E2103` when the packed copy does not read back equal.
pub fn seal(main: &Path, n: u64) -> Result<()> {
    let plain = plain_path(main, n);
    let packed = packed_path(main, n);
    let tmp = main.join(format!("{}.jsonl.zst.tmp", segment_name(n)));
    let bytes = std::fs::read(&plain).map_err(|e| Error::io(&plain, e))?;
    {
        let file = std::fs::File::create(&tmp).map_err(|e| Error::io(&tmp, e))?;
        let mut enc =
            zstd::stream::write::Encoder::new(file, SEAL_LEVEL).map_err(|e| Error::io(&tmp, e))?;
        enc.include_checksum(true).map_err(|e| Error::io(&tmp, e))?;
        enc.write_all(&bytes).map_err(|e| Error::io(&tmp, e))?;
        let file = enc.finish().map_err(|e| Error::io(&tmp, e))?;
        file.sync_all().map_err(|e| Error::io(&tmp, e))?;
    }
    let back = std::fs::read(&tmp).map_err(|e| Error::io(&tmp, e))?;
    let decoded = zstd::decode_all(&back[..]).map_err(|e| Error::io(&tmp, e))?;
    if decoded != bytes {
        let _ = std::fs::remove_file(&tmp);
        return Err(Error::code(
            Code::E2103,
            format!(
                "packed copy of {} does not read back equal",
                segment_name(n)
            ),
        ));
    }
    std::fs::rename(&tmp, &packed).map_err(|e| Error::io(&packed, e))?;
    fsync_dir(main)?;
    std::fs::remove_file(&plain).map_err(|e| Error::io(&plain, e))?;
    fsync_dir(main)
}

/// Finish a sealing that stopped before the removal (11.21 step 0): when
/// both forms exist, the plain file is authoritative; the packed copy is
/// kept when it decodes to the same bytes and made again otherwise.
///
/// # Errors
/// As [`seal`].
pub fn resume_seal(main: &Path, n: u64) -> Result<()> {
    let (plain, packed) = forms(main, n);
    if !(plain && packed) {
        return Ok(());
    }
    let plain_bytes =
        std::fs::read(plain_path(main, n)).map_err(|e| Error::io(plain_path(main, n), e))?;
    let same = std::fs::read(packed_path(main, n))
        .ok()
        .and_then(|c| zstd::decode_all(&c[..]).ok())
        .is_some_and(|d| d == plain_bytes);
    if same {
        let p = plain_path(main, n);
        std::fs::remove_file(&p).map_err(|e| Error::io(&p, e))?;
        fsync_dir(main)
    } else {
        seal(main, n)
    }
}

/// Decode a reader fully (used by tests and the object store).
///
/// # Errors
/// `std::io::Error` from zstd.
pub fn decode_all(mut r: impl Read) -> std::io::Result<Vec<u8>> {
    let mut buf = Vec::new();
    r.read_to_end(&mut buf)?;
    zstd::decode_all(&buf[..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_round_trip_and_frames() {
        let payload = b"{\"a\":1}\n{\"b\":2}\n";
        let h = BatchHeader {
            b: 1,
            n: 2,
            len: payload.len() as u64,
            h: payload_hash(payload),
            epoch: 1,
        };
        let line = h.line();
        assert!(line.starts_with("{\"b\":1,\"epoch\":1,\"h\":\"sha256:"));
        assert_eq!(
            BatchHeader::parse(line.trim_end().as_bytes()),
            Some(h.clone())
        );
        assert_eq!(BatchHeader::parse(b"{\"b\":1}"), None);
        let mut seg = line.clone().into_bytes();
        seg.extend_from_slice(payload);
        let second = BatchHeader { b: 2, ..h.clone() };
        seg.extend_from_slice(second.line().as_bytes());
        seg.extend_from_slice(payload);
        let fs: Vec<_> = frames(&seg, 0, seg.len() as u64).collect();
        assert_eq!(fs.len(), 2);
        let f0 = fs[0].as_ref().unwrap();
        assert_eq!(f0.header_offset, 0);
        assert_eq!(f0.payload_offset, line.len() as u64);
        assert_eq!(f0.payload, payload);
        assert_eq!(split_lines(f0.payload).unwrap().len(), 2);
        assert_eq!(split_lines(b"abc"), None);
        let f1 = fs[1].as_ref().unwrap();
        assert_eq!(f1.header.b, 2);
        assert_eq!(f1.end, seg.len() as u64);
        // Short: cut the second payload.
        let cut: Vec<_> = frames(&seg, 0, seg.len() as u64 - 3).collect();
        assert!(matches!(cut[1], Err(FrameError::Short { .. })));
        // Bad header.
        let bad: Vec<_> = frames(b"nonsense\n", 0, 9).collect();
        assert!(matches!(bad[0], Err(FrameError::BadHeader { offset: 0 })));
    }

    #[test]
    fn seal_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path();
        let bytes = b"{\"x\":1}\n".repeat(1000);
        std::fs::write(plain_path(main, 1), &bytes).unwrap();
        assert_eq!(load(main, 1).unwrap().unwrap().form, Form::Plain);
        seal(main, 1).unwrap();
        assert_eq!(forms(main, 1), (false, true));
        let l = load(main, 1).unwrap().unwrap();
        assert_eq!(l.form, Form::Packed);
        assert_eq!(l.bytes, bytes);
        assert!(load(main, 2).unwrap().is_none());
        // Resume: both forms, packed stale.
        std::fs::write(plain_path(main, 1), b"other\n").unwrap();
        resume_seal(main, 1).unwrap();
        assert_eq!(forms(main, 1), (false, true));
        assert_eq!(load(main, 1).unwrap().unwrap().bytes, b"other\n");
        // Corrupt packed bytes.
        let mut c = std::fs::read(packed_path(main, 1)).unwrap();
        let mid = c.len() / 2;
        c[mid] ^= 0xff;
        std::fs::write(packed_path(main, 1), &c).unwrap();
        assert!(matches!(load(main, 1), Err(LoadError::PackedCorrupt(1, _))));
    }
}
