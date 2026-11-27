//! `file.panels` (section 9; item 49; item 74): the panel names of an image
//! version, replacing the list.

use tm_formats::id::{EventId, VersionHandle};
use tm_owner_cap::OwnerCap;

use crate::ops::{
    Core, file_by_handle_or_path, latest_version, owner, version_by_handle, version_bytes,
};
use crate::{Code, Error, Result};

/// `tm file panels HANDLE NAME...` (`F<n>@v<m>` or `F<n>` for its latest
/// version).
///
/// # Errors
/// `E3301` not an image; `E3201` no version; `E1002` an empty name;
/// `E6001` unknown file; `E3203` bytes not available; the lock's and the
/// append's.
pub fn set(core: &mut Core, cap: &OwnerCap, version: &str, names: &[String]) -> Result<EventId> {
    let (identity, row) = match VersionHandle::parse(version) {
        Some(h) => {
            let file = file_by_handle_or_path(core, &h.file.to_string())?;
            let mut tx = core.tx()?;
            let v = version_by_handle(&mut tx, &h)?;
            tx.rollback()?;
            (
                file.identity,
                v.ok_or_else(|| Error::code(Code::E6001, format!("{version} does not exist")))?,
            )
        }
        None => {
            let file = file_by_handle_or_path(core, version)?;
            let mut tx = core.tx()?;
            let v = latest_version(&mut tx, file.identity.file)?;
            tx.rollback()?;
            (
                file.identity,
                v.ok_or_else(|| {
                    Error::code(
                        Code::E3201,
                        format!("cannot panels this file: {version} has no version yet"),
                    )
                })?,
            )
        }
    };
    let bytes = version_bytes(core.env(), &identity, &row)?;
    let is_image = tm_files::image::is_image_bytes(&bytes[..bytes.len().min(16)]);
    let mut state = identity.state();
    state.latest = Some(row.kept());
    let ev = tm_files::image::set_panels(state, &row.handle, is_image, names)?;
    let mut locked = core.lock(|_, _| {})?;
    let committed = locked.commit(&owner(cap), &[ev])?;
    locked.release()?;
    Ok(committed.own()[0].clone())
}
