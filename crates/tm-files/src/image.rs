//! Image versions and their panel names (item 49; 7.3's `panel` citation):
//! an image is recognized by its signature, and `file.panels_set` stores
//! the names on the version.

use std::io::Read;
use std::path::Path;

use serde_json::json;
use tm_formats::events::{EventType, KnownEvent};
use tm_formats::id::VersionHandle;

use crate::attributes::{Action, State};
use crate::{Error, Result, event};

/// True when `bytes` start with the PNG, JPEG, GIF, TIFF, BMP, or WebP
/// signature (item 49).
#[must_use]
pub fn is_image_bytes(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        || bytes.starts_with(b"\xff\xd8\xff")
        || bytes.starts_with(b"GIF87a")
        || bytes.starts_with(b"GIF89a")
        || bytes.starts_with(b"II*\0")
        || bytes.starts_with(b"MM\0*")
        || bytes.starts_with(b"BM")
        || (bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP")
}

/// True when the file at `path` starts with an image signature.
///
/// # Errors
/// `Error::Io`.
pub fn is_image(path: &Path) -> Result<bool> {
    let mut f = std::fs::File::open(path).map_err(|e| Error::io(path, e))?;
    let mut head = [0u8; 16];
    let mut n = 0;
    while n < head.len() {
        match f.read(&mut head[n..]) {
            Ok(0) => break,
            Ok(k) => n += k,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(Error::io(path, e)),
        }
    }
    Ok(is_image_bytes(&head[..n]))
}

/// Set the panel names of an image version (`tm file panels`), replacing
/// the list: the `file.panels_set` event.
///
/// # Errors
/// `E3301` when the version is not an image; `E3201` when the identity has
/// no version; `E1002` for an empty name.
pub fn set_panels(
    state: State,
    version: &VersionHandle,
    is_image: bool,
    names: &[String],
) -> Result<KnownEvent> {
    state.apply(Action::SetPanels { is_image })?;
    event(
        EventType::FilePanelsSet,
        &json!({"version": version.to_string(), "panels": names}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signatures() {
        assert!(is_image_bytes(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR"));
        assert!(is_image_bytes(b"\xff\xd8\xff\xe0\0\x10JFIF"));
        assert!(is_image_bytes(b"GIF89a\x01\0"));
        assert!(is_image_bytes(b"II*\0\x08\0\0\0"));
        assert!(is_image_bytes(b"MM\0*\0\0\0\x08"));
        assert!(is_image_bytes(b"BM\x36\0\0\0"));
        assert!(is_image_bytes(b"RIFF\x24\0\0\0WEBPVP8 "));
        assert!(!is_image_bytes(b"%PDF-1.4"));
        assert!(!is_image_bytes(b"gene_id\tlog2FoldChange"));
        assert!(!is_image_bytes(b""));
    }
}
