//! The `tm doctor` and `project.init` probes (item 30a; section 9 `doctor`
//! row; 3.1; `E2003`): filesystem type by `statfs` magic, mount options,
//! exclusive create, two-process rename visibility, locality, writability.

use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use crate::Error;
use crate::racy::random_hex;

/// What `statfs` says a path's filesystem is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FsType {
    /// The `f_type` magic.
    pub magic: u64,
    /// A name for the magic when it is a known one.
    pub name: Option<&'static str>,
    /// True for the network set: NFS, Lustre, GPFS, CIFS and SMB, FUSE.
    pub network: bool,
}

const NETWORK: &[(u64, &str)] = &[
    (0x6969, "nfs"),
    (0x0BD0_0BD0, "lustre"),
    (0x4750_4653, "gpfs"),
    (0xFF53_4D42, "cifs"),
    (0xFE53_4D42, "smb2"),
    (0x517B, "smb"),
    (0x6573_5546, "fuse"),
];

const LOCAL: &[(u64, &str)] = &[
    (0xEF53, "ext4"),
    (0x5846_5342, "xfs"),
    (0x9123_683E, "btrfs"),
    (0x0102_1994, "tmpfs"),
    (0x794C_7630, "overlay"),
    (0x2FC1_2FC1, "zfs"),
    (0xF2F5_2010, "f2fs"),
    (0x8584_58F6, "ramfs"),
];

/// The filesystem type at `path`.
///
/// # Errors
/// `Error::Io` when `statfs` fails.
pub fn fs_type(path: &Path) -> Result<FsType, Error> {
    let st = rustix::fs::statfs(path).map_err(|e| Error::io(path, e.into()))?;
    #[allow(
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation,
        clippy::unnecessary_cast,
        reason = "f_type is a signed word on some targets; the magic is its bit pattern"
    )]
    let magic = st.f_type as u64;
    let network = NETWORK.iter().find(|(m, _)| *m == magic);
    let local = LOCAL.iter().find(|(m, _)| *m == magic);
    Ok(FsType {
        magic,
        name: network.or(local).map(|(_, n)| *n),
        network: network.is_some(),
    })
}

/// True when `path` is on a network filesystem (3.4 refuses those for the
/// index and stat cache; `tm doctor` reports it for the record).
///
/// # Errors
/// `Error::Io` when `statfs` fails.
pub fn is_network(path: &Path) -> Result<bool, Error> {
    Ok(fs_type(path)?.network)
}

/// One line of `/proc/self/mounts`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mount {
    /// The device or export.
    pub source: String,
    /// The mount point.
    pub target: PathBuf,
    /// The filesystem type name as mounted.
    pub fstype: String,
    /// The mount options, comma-separated as the kernel reports them.
    pub options: String,
}

/// The mount `path` lives under: the longest mount point that is a prefix
/// of its canonical form. `None` when `/proc/self/mounts` is unreadable.
///
/// # Errors
/// `Error::Io` when `path` cannot be canonicalized.
pub fn mount_of(path: &Path) -> Result<Option<Mount>, Error> {
    let canon = std::fs::canonicalize(path).map_err(|e| Error::io(path, e))?;
    let Ok(text) = std::fs::read_to_string("/proc/self/mounts") else {
        return Ok(None);
    };
    let mut best: Option<Mount> = None;
    for line in text.lines() {
        let f: Vec<&str> = line.split(' ').collect();
        if f.len() < 4 {
            continue;
        }
        let target = PathBuf::from(unescape(f[1]));
        if canon.starts_with(&target)
            && best
                .as_ref()
                .is_none_or(|b| target.as_os_str().len() >= b.target.as_os_str().len())
        {
            best = Some(Mount {
                source: unescape(f[0]),
                target,
                fstype: f[2].to_owned(),
                options: f[3].to_owned(),
            });
        }
    }
    Ok(best)
}

/// `/proc/self/mounts` escapes space, tab, newline, and backslash as octal.
fn unescape(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\'
            && i + 3 < b.len()
            && b[i + 1..i + 4].iter().all(|c| (b'0'..=b'7').contains(c))
        {
            let v = (b[i + 1] - b'0') * 64 + (b[i + 2] - b'0') * 8 + (b[i + 3] - b'0');
            out.push(v);
            i += 4;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// True when `O_EXCL` creation is exclusive in `dir`: a second create of the
/// same name fails with "already exists".
///
/// # Errors
/// `Error::Io` when the first create fails.
pub fn exclusive_create_probe(dir: &Path) -> Result<bool, Error> {
    let path = dir.join(format!(".toulmin-probe-{}", random_hex()));
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| Error::io(&path, e))?;
    let second = OpenOptions::new().write(true).create_new(true).open(&path);
    let _ = std::fs::remove_file(&path);
    Ok(matches!(second, Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists))
}

/// Environment variable naming the source of the child's rename.
pub const PROBE_FROM: &str = "TM_PROBE_FROM";
/// Environment variable naming the target of the child's rename.
pub const PROBE_TO: &str = "TM_PROBE_TO";

/// Two-process rename visibility (3.1; 2.3): this process writes a file in
/// `dir`, `child` (run with [`PROBE_FROM`] and [`PROBE_TO`] set) renames it,
/// and this process must then see the new name with the same content and
/// not the old one, after a directory refresh. `Ok(false)` when the rename
/// is not visible within `timeout` or the child fails.
///
/// # Errors
/// `Error::Io` when the probe files cannot be written or the child cannot
/// be spawned.
pub fn rename_visibility_probe(
    dir: &Path,
    child: &mut Command,
    timeout: Duration,
) -> Result<bool, Error> {
    let tag = random_hex();
    let from = dir.join(format!(".toulmin-probe-{tag}.a"));
    let to = dir.join(format!(".toulmin-probe-{tag}.b"));
    let content = format!("toulmin rename probe {tag}\n");
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&from)
        .map_err(|e| Error::io(&from, e))?;
    f.write_all(content.as_bytes())
        .map_err(|e| Error::io(&from, e))?;
    f.sync_all().map_err(|e| Error::io(&from, e))?;
    drop(f);
    let mut proc = child
        .env(PROBE_FROM, &from)
        .env(PROBE_TO, &to)
        .spawn()
        .map_err(|e| Error::io(dir, e))?;
    let start = Instant::now();
    let status = loop {
        if let Some(s) = proc.try_wait().map_err(|e| Error::io(dir, e))? {
            break Some(s);
        }
        if start.elapsed() > timeout {
            let _ = proc.kill();
            let _ = proc.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let visible = status.is_some_and(|s| s.success()) && {
        // A fresh opendir and closedir invalidates most clients' caches (3.1).
        let _ = std::fs::read_dir(dir).map(Iterator::count);
        let mut seen = String::new();
        let read_ok = std::fs::File::open(&to)
            .and_then(|mut f| f.read_to_string(&mut seen))
            .is_ok();
        read_ok && seen == content && !from.exists()
    };
    let _ = std::fs::remove_file(&from);
    let _ = std::fs::remove_file(&to);
    Ok(visible)
}

/// The child's half of [`rename_visibility_probe`], for a binary that is
/// told to act as the probe child: rename `$TM_PROBE_FROM` to `$TM_PROBE_TO`.
///
/// # Errors
/// `Error::Io` when the variables are absent or the rename fails.
pub fn rename_probe_child() -> Result<(), Error> {
    let from = std::env::var_os(PROBE_FROM).map(PathBuf::from);
    let to = std::env::var_os(PROBE_TO).map(PathBuf::from);
    match (from, to) {
        (Some(from), Some(to)) => std::fs::rename(&from, &to).map_err(|e| Error::io(&from, e)),
        _ => Err(Error::io(
            "",
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "TM_PROBE_FROM and TM_PROBE_TO are not set",
            ),
        )),
    }
}

/// True when a file can be created and removed in `dir`.
#[must_use]
pub fn is_writable(dir: &Path) -> bool {
    let path = dir.join(format!(".toulmin-probe-{}", random_hex()));
    let ok = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .is_ok();
    if ok {
        let _ = std::fs::remove_file(&path);
    }
    ok
}

/// `tm doctor`'s locality test for the index and stat cache (3.4): a local
/// filesystem that is writable.
///
/// # Errors
/// `Error::Io` when `statfs` fails.
pub fn is_local_writable_directory(dir: &Path) -> Result<bool, Error> {
    Ok(dir.is_dir() && !is_network(dir)? && is_writable(dir))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probes_on_tmp() {
        let dir = std::env::temp_dir();
        let t = fs_type(&dir).unwrap();
        assert!(t.magic != 0);
        assert!(exclusive_create_probe(&dir).unwrap());
        assert!(is_writable(&dir));
        assert!(is_local_writable_directory(&dir).unwrap() || t.network);
        let m = mount_of(&dir).unwrap();
        if let Some(m) = m {
            assert!(
                dir.starts_with(&m.target)
                    || std::fs::canonicalize(&dir).unwrap().starts_with(&m.target)
            );
        }
        assert_eq!(unescape("/mnt/with\\040space"), "/mnt/with space");
        assert_eq!(unescape("plain"), "plain");
        assert_eq!(unescape("x\\"), "x\\");
    }

    #[test]
    fn rename_probe_with_mv() {
        let dir = std::env::temp_dir();
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "mv \"$TM_PROBE_FROM\" \"$TM_PROBE_TO\""]);
        assert!(rename_visibility_probe(&dir, &mut cmd, Duration::from_secs(10)).unwrap());
        let mut nothing = Command::new("sh");
        nothing.args(["-c", "true"]);
        assert!(!rename_visibility_probe(&dir, &mut nothing, Duration::from_secs(10)).unwrap());
        assert!(rename_probe_child().is_err());
    }
}
