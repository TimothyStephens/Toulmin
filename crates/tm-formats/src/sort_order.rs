//! Text sort order (design 11.10, item 22): binary order of NFC-normalized
//! UTF-8 bytes. No locale collation anywhere.

use std::cmp::Ordering;

use unicode_normalization::{UnicodeNormalization, is_nfc};

/// NFC form of `s`.
#[must_use]
pub fn nfc(s: &str) -> String {
    if is_nfc(s) {
        s.to_owned()
    } else {
        s.nfc().collect()
    }
}

/// Compare two strings in 11.10 order.
#[must_use]
pub fn cmp_text(a: &str, b: &str) -> Ordering {
    if is_nfc(a) && is_nfc(b) {
        a.as_bytes().cmp(b.as_bytes())
    } else {
        nfc(a).as_bytes().cmp(nfc(b).as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_after_nfc() {
        // "e" + combining acute is NFC "é" (U+00E9), which sorts after "z".
        assert_eq!(cmp_text("e\u{301}", "z"), Ordering::Greater);
        assert_eq!(cmp_text("e\u{301}", "\u{e9}"), Ordering::Equal);
        assert_eq!(cmp_text("B", "a"), Ordering::Less);
    }
}
