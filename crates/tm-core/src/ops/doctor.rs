//! `doctor` (section 9's `doctor` row; item 74; item 85): item 30a's
//! probes on the record path and the cache directory, plus glibc and
//! kernel versions and the `tm-duckdb` facts. The CLI renders the result
//! (item 85).

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use serde_json::{Value, json};
use tm_fsfacts::probes;

use crate::Result;
use crate::index::locate::Locality;
use crate::ops::Core;

/// One probed directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirReport {
    /// What was probed.
    pub role: &'static str,
    /// The directory.
    pub path: std::path::PathBuf,
    /// Filesystem type name when known.
    pub fs_type: Option<String>,
    /// Mount source and options when known.
    pub mount: Option<(String, String)>,
    /// On a network filesystem.
    pub network: bool,
    /// Writable.
    pub writable: bool,
    /// Atomic exclusive create works.
    pub exclusive_create: Option<bool>,
    /// A rename is visible to a second process (`None`: not probed).
    pub rename_visible: Option<bool>,
}

/// The doctor's result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// The record and cache directories.
    pub dirs: Vec<DirReport>,
    /// `uname -r` or `/proc/version`.
    pub kernel: Option<String>,
    /// `getconf GNU_LIBC_VERSION`.
    pub glibc: Option<String>,
    /// `tm-duckdb` facts: version, extensions, build flavour.
    pub duckdb: Option<(String, Vec<String>, String)>,
    /// The lock holder, if any.
    pub lock_holder: Option<Value>,
}

impl Report {
    /// Every directory passes its checks.
    #[must_use]
    pub fn ok(&self) -> bool {
        self.dirs.iter().all(|d| match d.role {
            "record" => {
                d.writable && d.exclusive_create == Some(true) && d.rename_visible != Some(false)
            }
            _ => !d.network && d.writable,
        })
    }

    /// The `--json` rendering.
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "ok": self.ok(),
            "dirs": self.dirs.iter().map(|d| json!({
                "role": d.role,
                "path": d.path.to_string_lossy(),
                "fs_type": d.fs_type,
                "mount": d.mount.as_ref().map(|(s, o)| json!({"source": s, "options": o})),
                "network": d.network,
                "writable": d.writable,
                "exclusive_create": d.exclusive_create,
                "rename_visible": d.rename_visible,
            })).collect::<Vec<_>>(),
            "kernel": self.kernel,
            "glibc": self.glibc,
            "duckdb": self.duckdb.as_ref().map(|(v, e, b)| json!({"version": v, "extensions": e, "build": b})),
            "lock_holder": self.lock_holder,
        })
    }
}

fn probe_dir(role: &'static str, path: &Path, probe_child: Option<&mut Command>) -> DirReport {
    let loc = Locality::probe(path);
    let mount = probes::mount_of(path)
        .ok()
        .flatten()
        .map(|m| (m.source, m.options));
    let exclusive_create = probes::exclusive_create_probe(path).ok();
    let rename_visible = probe_child
        .and_then(|c| probes::rename_visibility_probe(path, c, Duration::from_secs(30)).ok());
    DirReport {
        role,
        path: path.to_path_buf(),
        fs_type: loc.fs_type,
        mount,
        network: loc.network,
        writable: loc.writable,
        exclusive_create,
        rename_visible,
    }
}

fn command_line(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).args(args).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    (!text.is_empty()).then_some(text)
}

/// Run the doctor on an open project.
///
/// # Errors
/// `Error::Io` when the lock file does not read.
pub fn doctor(core: &Core, probe_child: Option<&mut Command>) -> Result<Report> {
    let record_dir = core.project.record.root().to_path_buf();
    let dirs = vec![
        probe_dir("record", &record_dir, probe_child),
        probe_dir("index", &core.project.index_dir(), None),
    ];
    let kernel = command_line("uname", &["-r"]).or_else(|| {
        std::fs::read_to_string("/proc/version")
            .ok()
            .map(|s| s.trim().to_owned())
    });
    let glibc = command_line("getconf", &["GNU_LIBC_VERSION"]);
    let duckdb = tm_duckdb::open(core.engine_settings())
        .and_then(|e| e.facts())
        .ok()
        .map(|f| (f.version, f.extensions, f.build.to_owned()));
    let lock_holder = core.record().lock_status()?.map(|h| h.to_json());
    Ok(Report {
        dirs,
        kernel,
        glibc,
        duckdb,
        lock_holder,
    })
}
