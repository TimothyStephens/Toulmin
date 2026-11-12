//! Event scans (item 37): all, after a position, by type, by position
//! `(segment, offset)` (11.3), and by event ID as a linear scan. The Core
//! resolves an ID to a position through the index's `events` table and
//! reads that position; the Record never reads the index.

use std::collections::VecDeque;

use tm_formats::envelope::{Line, read_line};
use tm_formats::id::EventId;

use crate::head::Head;
use crate::segment::{self, Frame, FrameError, Loaded, split_lines};
use crate::{Code, Error, Record, Result};

/// Where an event's line starts (11.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Position {
    /// Segment number, from 1.
    pub segment: u64,
    /// Byte offset of the line's first byte in the segment's uncompressed
    /// bytes.
    pub offset: u64,
}

/// One event as stored.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredEvent {
    /// Where.
    pub position: Position,
    /// `seq`.
    pub seq: u64,
    /// `batch`.
    pub batch: u64,
    /// `epoch`.
    pub epoch: u64,
    /// The validated line.
    pub line: Line,
}

impl StoredEvent {
    /// The event ID.
    #[must_use]
    pub fn id(&self) -> &EventId {
        &self.line.id
    }

    /// The `type` as written.
    #[must_use]
    pub fn type_name(&self) -> &str {
        self.line.envelope["type"].as_str().unwrap_or_default()
    }

    /// `prev`.
    #[must_use]
    pub fn prev(&self) -> Option<EventId> {
        self.line.envelope["prev"].as_str().and_then(EventId::parse)
    }
}

fn corrupt(msg: impl Into<String>) -> Error {
    Error::code(Code::E2103, msg)
}

/// Parse one stored line at a position.
pub(crate) fn parse_stored(position: Position, bytes: &[u8]) -> Result<StoredEvent> {
    let text = std::str::from_utf8(bytes).map_err(|_| {
        corrupt(format!(
            "segment {} offset {}: line is not UTF-8",
            position.segment, position.offset
        ))
    })?;
    let line = read_line(text).map_err(|e| {
        corrupt(format!(
            "segment {} offset {}: {e}",
            position.segment, position.offset
        ))
    })?;
    let int = |k: &str| line.envelope[k].as_u64();
    let (Some(seq), Some(batch), Some(epoch)) = (int("seq"), int("batch"), int("epoch")) else {
        return Err(corrupt(format!(
            "segment {} offset {}: not a main-chain envelope",
            position.segment, position.offset
        )));
    };
    Ok(StoredEvent {
        position,
        seq,
        batch,
        epoch,
        line,
    })
}

/// The events of one frame, with their positions.
pub(crate) fn frame_events(segment: u64, frame: &Frame<'_>) -> Result<Vec<StoredEvent>> {
    let lines = split_lines(frame.payload).ok_or_else(|| {
        corrupt(format!(
            "segment {segment} offset {}: payload does not end with a newline",
            frame.payload_offset
        ))
    })?;
    let mut out = Vec::with_capacity(lines.len());
    let mut offset = frame.payload_offset;
    for l in lines {
        out.push(parse_stored(Position { segment, offset }, l)?);
        offset += l.len() as u64 + 1;
    }
    Ok(out)
}

/// The committed end of a loaded segment.
fn committed_end(head: &Head, segment: u64, loaded: &Loaded) -> u64 {
    if segment == head.segment {
        head.offset.min(loaded.bytes.len() as u64)
    } else {
        loaded.bytes.len() as u64
    }
}

/// An iterator over the committed events, in chain order.
pub struct Scan<'a> {
    record: &'a Record,
    head: Head,
    next_segment: u64,
    pending: VecDeque<StoredEvent>,
    after: Option<Position>,
    done: bool,
}

impl Iterator for Scan<'_> {
    type Item = Result<StoredEvent>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(e) = self.pending.pop_front() {
                return Some(Ok(e));
            }
            if self.done || self.next_segment > self.head.segment {
                self.done = true;
                return None;
            }
            let n = self.next_segment;
            self.next_segment += 1;
            match self.load_segment(n) {
                Ok(()) => {}
                Err(e) => {
                    self.done = true;
                    return Some(Err(e));
                }
            }
        }
    }
}

impl Scan<'_> {
    fn load_segment(&mut self, n: u64) -> Result<()> {
        let main = self.record.main_dir();
        let Some(loaded) = segment::load(&main, n)? else {
            if n == self.head.segment && self.head.offset == 0 {
                return Ok(());
            }
            return Err(corrupt(format!("segment {n} is missing")));
        };
        let end = committed_end(&self.head, n, &loaded);
        if n == self.head.segment && (loaded.bytes.len() as u64) < self.head.offset {
            return Err(corrupt(format!(
                "HEAD.offset {} is beyond the end of segment {n}: head_offset_mismatch",
                self.head.offset
            )));
        }
        for frame in segment::frames(&loaded.bytes, 0, end) {
            let frame = frame.map_err(|e| match e {
                FrameError::BadHeader { offset } => {
                    corrupt(format!("segment {n} offset {offset}: not a batch header"))
                }
                FrameError::Short {
                    offset,
                    header,
                    have,
                } => corrupt(format!(
                    "segment {n} offset {offset}: batch {} declares {} bytes, {have} present",
                    header.b, header.len
                )),
            })?;
            for e in frame_events(n, &frame)? {
                if self.after.is_none_or(|p| e.position > p) {
                    self.pending.push_back(e);
                }
            }
        }
        Ok(())
    }
}

impl Record {
    /// Every committed event, in order.
    ///
    /// # Errors
    /// `E2103` from the iterator on corruption; `Error::Io`.
    pub fn events(&self) -> Result<Scan<'_>> {
        let head = self.head()?;
        Ok(Scan {
            record: self,
            head,
            next_segment: 1,
            pending: VecDeque::new(),
            after: None,
            done: false,
        })
    }

    /// Every committed event after `position`.
    ///
    /// # Errors
    /// As [`Record::events`].
    pub fn events_after(&self, position: Position) -> Result<Scan<'_>> {
        let head = self.head()?;
        Ok(Scan {
            record: self,
            head,
            next_segment: position.segment.max(1),
            pending: VecDeque::new(),
            after: Some(position),
            done: false,
        })
    }

    /// Every committed event of one `type`.
    ///
    /// # Errors
    /// As [`Record::events`].
    pub fn events_of_type<'a>(
        &'a self,
        ty: &'a str,
    ) -> Result<impl Iterator<Item = Result<StoredEvent>> + 'a> {
        Ok(self.events()?.filter(move |e| match e {
            Ok(e) => e.type_name() == ty,
            Err(_) => true,
        }))
    }

    /// The event at a position (the Core's resolve-by-ID path, section 9).
    ///
    /// # Errors
    /// `E2103` when no committed event line starts there; `Error::Io`.
    pub fn read_at(&self, position: Position) -> Result<StoredEvent> {
        let main = self.main_dir();
        let head = self.head()?;
        if position.segment == 0 || position.segment > head.segment {
            return Err(corrupt(format!(
                "segment {} is not on the chain",
                position.segment
            )));
        }
        let loaded = segment::load(&main, position.segment)?
            .ok_or_else(|| corrupt(format!("segment {} is missing", position.segment)))?;
        let end = committed_end(&head, position.segment, &loaded);
        if position.offset >= end {
            return Err(corrupt(format!(
                "segment {} offset {} is beyond the committed end {end}",
                position.segment, position.offset
            )));
        }
        let start = usize::try_from(position.offset).unwrap_or(usize::MAX);
        let rest = &loaded.bytes[start..usize::try_from(end).unwrap_or(usize::MAX)];
        let nl = rest.iter().position(|b| *b == b'\n').ok_or_else(|| {
            corrupt(format!(
                "segment {} offset {}: no line ends before the committed end",
                position.segment, position.offset
            ))
        })?;
        parse_stored(position, &rest[..nl])
    }

    /// Find an event by ID with a linear scan from the first segment (the
    /// fallback when the index is absent).
    ///
    /// # Errors
    /// As [`Record::events`].
    pub fn find_by_id(&self, id: &EventId) -> Result<Option<StoredEvent>> {
        for e in self.events()? {
            let e = e?;
            if e.id() == id {
                return Ok(Some(e));
            }
        }
        Ok(None)
    }
}
