//! Where the index lives (3.4; item 76).
//!
//! `$XDG_CACHE_HOME/toulmin/<project-id>/<hostname>/index.sqlite` when the
//! variable is set and names a local, writable filesystem (as `tm doctor`
//! judges it, item 30a), otherwise `$TMPDIR/toulmin-$UID/<project-id>/`,
//! then `/tmp/toulmin-$UID/<project-id>/`. The same directory holds the
//! stat cache (a table of the database) and the parse cache
//! (`parse-cache/`). A network filesystem is refused for all three.

use std::path::{Path, PathBuf};

use tm_fsfacts::probes;

/// The cache root the environment gives (3.4's order), before the
/// `<project-id>/<hostname>/` part.
#[must_use]
pub fn cache_root(uid: u32) -> PathBuf {
    if let Some(x) = std::env::var_os("XDG_CACHE_HOME").filter(|v| !v.is_empty()) {
        let root = PathBuf::from(x).join("toulmin");
        if is_local(&root) {
            return root;
        }
    }
    let base = std::env::var_os("TMPDIR")
        .filter(|v| !v.is_empty())
        .map_or_else(|| PathBuf::from("/tmp"), PathBuf::from);
    let under_tmp = base.join(format!("toulmin-{uid}"));
    if is_local(&under_tmp) {
        return under_tmp;
    }
    PathBuf::from("/tmp").join(format!("toulmin-{uid}"))
}

/// True when `dir` (created if absent) is on a local filesystem and
/// writable.
fn is_local(dir: &Path) -> bool {
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    probes::is_local_writable_directory(dir).unwrap_or(false)
}

/// What `tm doctor` reports about an index directory (3.4; item 85).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Locality {
    /// The directory.
    pub dir: PathBuf,
    /// Filesystem type name when known.
    pub fs_type: Option<String>,
    /// On a network filesystem (refused for the index and stat cache).
    pub network: bool,
    /// Writable.
    pub writable: bool,
}

impl Locality {
    /// Probe `dir`, creating it if absent.
    #[must_use]
    pub fn probe(dir: &Path) -> Locality {
        let _ = std::fs::create_dir_all(dir);
        let t = probes::fs_type(dir).ok();
        Locality {
            dir: dir.to_path_buf(),
            fs_type: t.as_ref().and_then(|t| t.name.map(str::to_owned)),
            network: t.as_ref().is_some_and(|t| t.network),
            writable: probes::is_writable(dir),
        }
    }

    /// Acceptable for the index and the stat cache.
    #[must_use]
    pub fn suitable(&self) -> bool {
        !self.network && self.writable
    }
}
