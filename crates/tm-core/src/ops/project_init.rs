//! `project.init` (section 9; OQ6; item 73; item 74).
//!
//! `tm init --record PATH` creates the record directory, `.toulmin/
//! project.json`, and the binding under the config directory, and writes
//! the first batch (`project.created`). `tm init --attach PATH` binds an
//! existing record to this checkout (a second checkout or another machine):
//! it checks `record.json`, writes the project file when absent, and adds
//! this repository root to the binding, writing no event.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use serde_json::json;
use tm_formats::envelope::SourceKind;
use tm_formats::events::{EventType, KnownEvent};
use tm_fsfacts::probes;
use tm_fsfacts::racy::random_hex;
use tm_owner_cap::OwnerCap;
use tm_record::Record;

use crate::ops::{Core, owner};
use crate::project::{Binding, Dirs, Project, ProjectFile, canonical_path};
use crate::{Code, Error, Result, index};

/// What `init` did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Initialized {
    /// The project ID.
    pub project_id: String,
    /// The record path.
    pub record: std::path::PathBuf,
    /// `project.created`'s ID (`None` for `--attach`).
    pub created_event: Option<String>,
    /// The rename-visibility probe ran and passed (`None` when no probe
    /// child was supplied, item 74).
    pub rename_probe: Option<bool>,
}

fn is_git_repository(root: &Path) -> bool {
    root.join(".git").exists()
}

/// The filesystem checks `project.init` makes on the record path (section
/// 9's `E2003`; item 30a): atomic exclusive create, and rename visibility
/// across two processes when `probe_child` is given.
///
/// # Errors
/// `E2003`; `Error::Io`.
pub fn check_record_filesystem(
    record_dir: &Path,
    probe_child: Option<&mut Command>,
) -> Result<Option<bool>> {
    if !probes::exclusive_create_probe(record_dir)? {
        return Err(Error::code(
            Code::E2003,
            format!(
                "{} does not support atomic exclusive create",
                record_dir.display()
            ),
        ));
    }
    match probe_child {
        Some(child) => {
            let ok = probes::rename_visibility_probe(record_dir, child, Duration::from_secs(30))?;
            if !ok {
                return Err(Error::code(
                    Code::E2003,
                    format!(
                        "{} does not make a rename visible to a second process",
                        record_dir.display()
                    ),
                ));
            }
            Ok(Some(true))
        }
        None => Ok(None),
    }
}

/// `tm init --record PATH`.
///
/// # Errors
/// `E2009` the project root is not a git repository; `E2001` record
/// exists; `E2002` path inside repository; `E2003` filesystem unsuitable;
/// `E2004` the repository already has a project file; `E2006` binding conflicts or config directory unwritable;
/// `Error::Io`.
pub fn init(
    cap: &OwnerCap,
    repo_root: &Path,
    record_path: &Path,
    dirs: Dirs,
    probe_child: Option<&mut Command>,
) -> Result<(Core, Initialized)> {
    let repo_root = canonical_path(repo_root)?;
    if !is_git_repository(&repo_root) {
        return Err(Error::code(
            Code::E2009,
            format!(
                "{} is not a git repository (no .git); Toulmin runs alongside git",
                repo_root.display()
            ),
        ));
    }
    if let Some(existing) = ProjectFile::read(&repo_root)? {
        return Err(Error::code(
            Code::E2004,
            format!(
                "{} already belongs to project {}; use `tm init --attach`",
                repo_root.display(),
                existing.project_id
            ),
        ));
    }
    std::fs::create_dir_all(record_path).map_err(|e| Error::io(record_path, e))?;
    let record_abs = canonical_path(record_path)?;
    if record_abs.starts_with(&repo_root) {
        return Err(Error::code(
            Code::E2002,
            format!(
                "the record path {} is inside the repository {}",
                record_abs.display(),
                repo_root.display()
            ),
        ));
    }
    let rename_probe = check_record_filesystem(&record_abs, probe_child)?;
    let project_id = format!("p-{}", &random_hex()[..16]);
    let owner_id = format!("o-{}", &random_hex()[..16]);
    let at = crate::now_at();
    let record = Record::create(&record_abs, &project_id, &owner_id, &at)?;
    let binding = Binding {
        project_id: project_id.clone(),
        record: record_abs.clone(),
        repos: vec![repo_root.clone()],
    };
    binding.write(&dirs)?;
    ProjectFile {
        project_id: project_id.clone(),
    }
    .write(&repo_root)?;
    drop(record);
    let project = Project::open(&repo_root, dirs)?;
    let mut core = Core::with_project(project, SourceKind::Cli)?;
    let root_hash = tm_fsfacts::hash::fingerprint_of(repo_root.to_string_lossy().as_bytes());
    let created = KnownEvent::new(
        EventType::ProjectCreated,
        &json!({
            "project_id": project_id,
            "owner_id": owner_id,
            "record_format": tm_formats::version::RECORD_FORMAT,
            "repo_root_hash": root_hash.as_str(),
        }),
    )?;
    let mut locked = core.lock(|_, _| {})?;
    let committed = locked.commit(&owner(cap), &[created])?;
    locked.release()?;
    let created_event = committed.own().first().map(|id| id.as_str().to_owned());
    Ok((
        core,
        Initialized {
            project_id,
            record: record_abs,
            created_event,
            rename_probe,
        },
    ))
}

/// `tm init --attach RECORD_PATH`: bind an existing record to this
/// checkout or machine. Writes the project file when the repository has
/// none, and adds this root to the binding; no event.
///
/// # Errors
/// `E2007` no `record.json`; `E2004` the repository's project file names
/// another project; `E2006` a binding for the project names another record;
/// `Error::Io`.
pub fn attach(
    _cap: &OwnerCap,
    repo_root: &Path,
    record_path: &Path,
    dirs: Dirs,
) -> Result<(Core, Initialized)> {
    let repo_root = canonical_path(repo_root)?;
    let record_abs = canonical_path(record_path)?;
    let record = Record::open(&record_abs)?;
    let project_id = record.header().project_id.clone();
    match ProjectFile::read(&repo_root)? {
        Some(p) if p.project_id != project_id => {
            return Err(Error::code(
                Code::E2004,
                format!(
                    "{} belongs to project {}, but the record at {} is project {}",
                    repo_root.display(),
                    p.project_id,
                    record_abs.display(),
                    project_id
                ),
            ));
        }
        Some(_) => {}
        None => ProjectFile {
            project_id: project_id.clone(),
        }
        .write(&repo_root)?,
    }
    let binding = match Binding::read(&dirs, &project_id)? {
        Some(mut b) => {
            if std::fs::canonicalize(&b.record).ok().as_deref() != Some(record_abs.as_path()) {
                return Err(Error::code(
                    Code::E2006,
                    format!(
                        "project {} is already bound to {}, not {}",
                        project_id,
                        b.record.display(),
                        record_abs.display()
                    ),
                ));
            }
            if !b.repos.contains(&repo_root) {
                b.repos.push(repo_root.clone());
            }
            b
        }
        None => Binding {
            project_id: project_id.clone(),
            record: record_abs.clone(),
            repos: vec![repo_root.clone()],
        },
    };
    binding.write(&dirs)?;
    drop(record);
    let project = Project::open(&repo_root, dirs)?;
    let mut core = Core::with_project(project, SourceKind::Cli)?;
    index::sync(&mut core.index, &core.project.record)?;
    Ok((
        core,
        Initialized {
            project_id,
            record: record_abs,
            created_event: None,
            rename_probe: None,
        },
    ))
}
