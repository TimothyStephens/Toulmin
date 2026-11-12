//! PDF page counts (item 49; 7.3's `page` citation): `lopdf` reads the
//! document and the page tree gives the count.

use std::path::Path;

use crate::{Code, Error, Result};

/// The number of pages of the PDF at `path`.
///
/// # Errors
/// `E3302` when `lopdf` finds no PDF page tree in the file.
pub fn page_count(path: &Path) -> Result<u64> {
    let not_pdf = |why: String| {
        Error::code(
            Code::E3302,
            format!("{}: not a PDF page tree: {why}", path.display()),
        )
    };
    let doc = lopdf::Document::load(path).map_err(|e| not_pdf(e.to_string()))?;
    let pages = doc.get_pages();
    if pages.is_empty() {
        return Err(not_pdf("no pages".into()));
    }
    Ok(pages.len() as u64)
}
