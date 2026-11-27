//! Per-command project resolution (item 73; OQ6; 3.4).
//!
//! A project is one repository plus one record directory, bound by two
//! small files: `.toulmin/project.json` in the repository root (the project
//! ID only, committed) and the binding `<config>/projects/<project-id>.json`
//! outside it (the record path and the repository roots it serves). Both
//! are canonical JSON plus one newline (11.3's small-file form). `tm`
//! refuses to run when the IDs in the two files and in `record.json`
//! disagree (`E2004`), or when the current repository root is not in the
//! binding (`E2005`); a bound path with no `record.json` is `E2007`, and a
//! record whose format is above this binary's opens read-only (`E2008` on
//! every write, which the Record enforces).

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tm_formats::canonical;
use tm_record::Record;

use crate::{Code, Error, Result};

/// The directories a command resolves against (3.4, OQ6), built from the
/// environment by [`Dirs::from_env`] or by hand in tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dirs {
    /// `<config>`: `$XDG_CONFIG_HOME/toulmin`, else `~/.config/toulmin`.
    pub config: PathBuf,
    /// The node-local cache root under which `<project-id>/<hostname>/`
    /// holds the index, the stat cache, and the parse cache (3.4).
    pub cache_root: PathBuf,
    /// This host's name, one index per host (3.4).
    pub hostname: String,
}

impl Dirs {
    /// From the environment: `XDG_CONFIG_HOME` or `HOME/.config` for the
    /// config directory; `XDG_CACHE_HOME/toulmin` when set, else
    /// `$TMPDIR/toulmin-$UID`, else `/tmp/toulmin-$UID` (3.4; `tm doctor`
    /// judges locality).
    #[must_use]
    pub fn from_env() -> Dirs {
        let config = match std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
            Some(x) => PathBuf::from(x).join("toulmin"),
            None => home().join(".config").join("toulmin"),
        };
        Dirs {
            config,
            cache_root: crate::index::locate::cache_root(uid()),
            hostname: hostname(),
        }
    }

    /// A self-contained layout under one directory (tests and the
    /// `index-rebuild` example).
    #[must_use]
    pub fn under(root: &Path) -> Dirs {
        Dirs {
            config: root.join("config"),
            cache_root: root.join("cache"),
            hostname: "localhost".to_owned(),
        }
    }

    /// `<config>/projects/<project-id>.json`.
    #[must_use]
    pub fn binding_path(&self, project_id: &str) -> PathBuf {
        self.config
            .join("projects")
            .join(format!("{project_id}.json"))
    }

    /// `<cache_root>/<project-id>/<hostname>/`: the index directory (3.4).
    #[must_use]
    pub fn index_dir(&self, project_id: &str) -> PathBuf {
        self.cache_root.join(project_id).join(&self.hostname)
    }
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .filter(|v| !v.is_empty())
        .map_or_else(|| PathBuf::from("/"), PathBuf::from)
}

fn uid() -> u32 {
    use std::os::unix::fs::MetadataExt as _;
    std::fs::metadata("/proc/self").map_or(0, |m| m.uid())
}

fn hostname() -> String {
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "localhost".to_owned())
}

/// `.toulmin/project.json`: `{"project":1,"project_id"}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectFile {
    /// The project ID.
    pub project_id: String,
}

impl ProjectFile {
    /// The file's path under a repository root.
    #[must_use]
    pub fn path(repo_root: &Path) -> PathBuf {
        repo_root.join(".toulmin").join("project.json")
    }

    /// Read it; `Ok(None)` when absent.
    ///
    /// # Errors
    /// `E2004` when it is present but malformed; `Error::Io`.
    pub fn read(repo_root: &Path) -> Result<Option<ProjectFile>> {
        let path = Self::path(repo_root);
        let Some(v) = read_small(&path)? else {
            return Ok(None);
        };
        let id = v
            .get("project_id")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .filter(|_| v.get("project").and_then(Value::as_u64) == Some(1));
        match id {
            Some(id) => Ok(Some(ProjectFile {
                project_id: id.to_owned(),
            })),
            None => Err(Error::code(
                Code::E2004,
                format!("{} is not a project file", path.display()),
            )),
        }
    }

    /// Write it (creating `.toulmin/`).
    ///
    /// # Errors
    /// `Error::Io`.
    pub fn write(&self, repo_root: &Path) -> Result<()> {
        let path = Self::path(repo_root);
        write_small(&path, &json!({"project": 1, "project_id": self.project_id}))
    }
}

/// The binding `<config>/projects/<project-id>.json`:
/// `{"binding":1,"project_id","record","repos"}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    /// The project ID.
    pub project_id: String,
    /// The absolute record path.
    pub record: PathBuf,
    /// The absolute repository roots this binding serves.
    pub repos: Vec<PathBuf>,
}

impl Binding {
    /// Read the binding for `project_id`; `Ok(None)` when absent.
    ///
    /// # Errors
    /// `E2004` when it is present but malformed; `Error::Io`.
    pub fn read(dirs: &Dirs, project_id: &str) -> Result<Option<Binding>> {
        let path = dirs.binding_path(project_id);
        let Some(v) = read_small(&path)? else {
            return Ok(None);
        };
        let bad = || Error::code(Code::E2004, format!("{} is not a binding", path.display()));
        if v.get("binding").and_then(Value::as_u64) != Some(1) {
            return Err(bad());
        }
        let id = v
            .get("project_id")
            .and_then(Value::as_str)
            .ok_or_else(bad)?;
        let record = v.get("record").and_then(Value::as_str).ok_or_else(bad)?;
        let repos = v
            .get("repos")
            .and_then(Value::as_array)
            .ok_or_else(bad)?
            .iter()
            .map(|r| r.as_str().map(PathBuf::from).ok_or_else(bad))
            .collect::<Result<Vec<_>>>()?;
        Ok(Some(Binding {
            project_id: id.to_owned(),
            record: PathBuf::from(record),
            repos,
        }))
    }

    /// Write the binding (creating `<config>/projects/`).
    ///
    /// # Errors
    /// `E2006` when the config directory cannot be created or written.
    pub fn write(&self, dirs: &Dirs) -> Result<()> {
        let path = dirs.binding_path(&self.project_id);
        let repos: Vec<String> = self
            .repos
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        write_small(
            &path,
            &json!({
                "binding": 1,
                "project_id": self.project_id,
                "record": self.record.to_string_lossy(),
                "repos": repos,
            }),
        )
        .map_err(|e| match e {
            Error::Io { path, source } => Error::code(
                Code::E2006,
                format!("cannot write the binding {}: {source}", path.display()),
            ),
            other => other,
        })
    }
}

fn read_small(path: &Path) -> Result<Option<Value>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(canonical::parse_bytes(&bytes).map_err(|e| {
            Error::code(
                Code::E2004,
                format!("{} is not canonical JSON: {e}", path.display()),
            )
        })?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::io(path, e)),
    }
}

fn write_small(path: &Path, value: &Value) -> Result<()> {
    use std::io::Write as _;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    let mut text = canonical::canonical(value)?;
    text.push('\n');
    let tmp = path.with_extension("json.tmp");
    let mut f = std::fs::File::create(&tmp).map_err(|e| Error::io(&tmp, e))?;
    f.write_all(text.as_bytes())
        .map_err(|e| Error::io(&tmp, e))?;
    f.sync_all().map_err(|e| Error::io(&tmp, e))?;
    std::fs::rename(&tmp, path).map_err(|e| Error::io(path, e))
}

/// Absolute, symlink-free form of a path that exists.
pub(crate) fn canonical_path(path: &Path) -> Result<PathBuf> {
    std::fs::canonicalize(path).map_err(|e| Error::io(path, e))
}

/// Walk up from `start` to the first directory holding
/// `.toulmin/project.json`.
#[must_use]
pub fn find_repo_root(start: &Path) -> Option<PathBuf> {
    let mut dir = Some(start.to_path_buf());
    while let Some(d) = dir {
        if ProjectFile::path(&d).is_file() {
            return Some(d);
        }
        dir = d.parent().map(Path::to_path_buf);
    }
    None
}

/// An open project: the repository, its ID, and the record directory.
#[derive(Debug)]
pub struct Project {
    /// The repository root, canonical.
    pub repo_root: PathBuf,
    /// The project ID.
    pub project_id: String,
    /// The open record.
    pub record: Record,
    /// The directories in use.
    pub dirs: Dirs,
}

impl Project {
    /// Resolve and open the project at `repo_root` (item 73).
    ///
    /// # Errors
    /// `E2004` when `.toulmin/project.json` or the binding is missing or
    /// the IDs disagree; `E2005` when the repository root is not in the
    /// binding; `E2007` when the bound path has no `record.json`;
    /// `E2103` on a malformed record header; `Error::Io`.
    pub fn open(repo_root: &Path, dirs: Dirs) -> Result<Project> {
        let repo_root = canonical_path(repo_root)?;
        let project = ProjectFile::read(&repo_root)?.ok_or_else(|| {
            Error::code(
                Code::E2004,
                format!(
                    "no .toulmin/project.json under {}: run `tm init`",
                    repo_root.display()
                ),
            )
        })?;
        let binding = Binding::read(&dirs, &project.project_id)?.ok_or_else(|| {
            Error::code(
                Code::E2004,
                format!(
                    "no binding for project {} under {}: run `tm init --attach RECORD_PATH`",
                    project.project_id,
                    dirs.config.join("projects").display()
                ),
            )
        })?;
        if binding.project_id != project.project_id {
            return Err(Error::code(
                Code::E2004,
                format!(
                    "project.json names {} but its binding names {}",
                    project.project_id, binding.project_id
                ),
            ));
        }
        let bound = binding
            .repos
            .iter()
            .any(|r| std::fs::canonicalize(r).is_ok_and(|c| c == repo_root));
        if !bound {
            return Err(Error::code(
                Code::E2005,
                format!(
                    "{} is not a repository root the binding for {} serves: run `tm init --attach {}`",
                    repo_root.display(),
                    project.project_id,
                    binding.record.display()
                ),
            ));
        }
        let record = Record::open(&binding.record)?;
        if record.header().project_id != project.project_id {
            return Err(Error::code(
                Code::E2004,
                format!(
                    "the record at {} is project {}, not {}",
                    binding.record.display(),
                    record.header().project_id,
                    project.project_id
                ),
            ));
        }
        Ok(Project {
            repo_root,
            project_id: project.project_id,
            record,
            dirs,
        })
    }

    /// The directory holding `index.sqlite` and the parse cache (3.4).
    #[must_use]
    pub fn index_dir(&self) -> PathBuf {
        self.dirs.index_dir(&self.project_id)
    }

    /// `index.sqlite`.
    #[must_use]
    pub fn index_path(&self) -> PathBuf {
        self.index_dir().join("index.sqlite")
    }

    /// `parse-cache/` beside the index (item 76).
    #[must_use]
    pub fn parse_cache_dir(&self) -> PathBuf {
        self.index_dir().join("parse-cache")
    }

    /// The absolute path of a repository-relative path.
    #[must_use]
    pub fn abs(&self, rel: &str) -> PathBuf {
        self.repo_root.join(rel)
    }

    /// The repository-relative, `/`-separated form of a path inside the
    /// project, given relative to the repository root or to `cwd`, or
    /// absolute.
    ///
    /// # Errors
    /// `E3001` when the path is not inside the repository (or does not
    /// exist, so it cannot be resolved).
    pub fn relative(&self, path: &Path, cwd: Option<&Path>) -> Result<String> {
        let joined = if path.is_absolute() {
            path.to_path_buf()
        } else if let Some(cwd) = cwd {
            cwd.join(path)
        } else {
            self.repo_root.join(path)
        };
        let full = match std::fs::canonicalize(&joined) {
            Ok(p) => p,
            Err(_) => {
                // A path that does not exist yet: resolve its parent.
                let parent = joined.parent().unwrap_or(&joined);
                let name = joined.file_name();
                match (std::fs::canonicalize(parent), name) {
                    (Ok(p), Some(n)) => p.join(n),
                    _ => {
                        return Err(Error::code(
                            Code::E3001,
                            format!("{} is not inside the project", path.display()),
                        ));
                    }
                }
            }
        };
        let rel = full.strip_prefix(&self.repo_root).map_err(|_| {
            Error::code(
                Code::E3001,
                format!(
                    "{} is outside the repository {}",
                    path.display(),
                    self.repo_root.display()
                ),
            )
        })?;
        let text = rel
            .to_str()
            .ok_or_else(|| Error::code(Code::E3003, format!("{} is not UTF-8", rel.display())))?;
        if text.is_empty() {
            return Err(Error::code(
                Code::E3001,
                "the repository root itself is not a file path",
            ));
        }
        Ok(text.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_and_binding_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let dirs = Dirs::under(tmp.path());
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        assert!(ProjectFile::read(&repo).unwrap().is_none());
        ProjectFile {
            project_id: "p-1".into(),
        }
        .write(&repo)
        .unwrap();
        assert_eq!(ProjectFile::read(&repo).unwrap().unwrap().project_id, "p-1");
        assert_eq!(find_repo_root(&repo.join("a/b")), Some(repo.clone()));
        let b = Binding {
            project_id: "p-1".into(),
            record: tmp.path().join("rec"),
            repos: vec![repo.clone()],
        };
        b.write(&dirs).unwrap();
        assert_eq!(Binding::read(&dirs, "p-1").unwrap(), Some(b));
        assert!(Binding::read(&dirs, "p-2").unwrap().is_none());
        // The record is missing: E2007 from the Record.
        let e = Project::open(&repo, dirs.clone()).unwrap_err();
        assert_eq!(e.catalog_code(), Some(Code::E2007));
        // A root the binding does not serve: E2005.
        let other = tmp.path().join("other");
        std::fs::create_dir_all(&other).unwrap();
        ProjectFile {
            project_id: "p-1".into(),
        }
        .write(&other)
        .unwrap();
        let e = Project::open(&other, dirs.clone()).unwrap_err();
        assert_eq!(e.catalog_code(), Some(Code::E2005));
        // No project file: E2004.
        let bare = tmp.path().join("bare");
        std::fs::create_dir_all(&bare).unwrap();
        let e = Project::open(&bare, dirs).unwrap_err();
        assert_eq!(e.catalog_code(), Some(Code::E2004));
    }
}
