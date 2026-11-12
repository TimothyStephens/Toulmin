//! PDF page counts (item 49; 7.3's `page` citation): `lopdf` reads the
//! document and the page tree gives the count.

use std::path::Path;

use crate::{Error, Result};

/// The number of pages of the PDF at `path`.
///
/// # Errors
/// [`Error::Pdf`] when `lopdf` cannot read the file as a PDF with a page
/// tree (no catalog code names this condition).
pub fn page_count(path: &Path) -> Result<u64> {
    let doc = lopdf::Document::load(path).map_err(|e| Error::Pdf {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;
    let pages = doc.get_pages();
    if pages.is_empty() {
        return Err(Error::Pdf {
            path: path.to_path_buf(),
            message: "no page tree".into(),
        });
    }
    Ok(pages.len() as u64)
}
