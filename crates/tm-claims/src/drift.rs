//! Drift (item 69; G11; 11.22): the claim's current revision's sentence
//! against a span's recorded text, both normalized: NFC, every run of
//! Unicode whitespace replaced by one space, trimmed. A span with inline
//! code is not checkable.

use unicode_normalization::UnicodeNormalization;

use crate::spans::Span;

/// A span's drift state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Drift {
    /// The normalized texts are equal.
    Same,
    /// The normalized texts differ: flag `text drifted`.
    Drifted,
    /// The span holds inline code.
    NotCheckable,
}

/// The 11.22 normalization for drift.
#[must_use]
pub fn normalize(s: &str) -> String {
    let nfc: String = s.nfc().collect();
    let mut out = String::with_capacity(nfc.len());
    let mut in_space = false;
    for c in nfc.chars() {
        if c.is_whitespace() {
            in_space = true;
        } else {
            if in_space && !out.is_empty() {
                out.push(' ');
            }
            in_space = false;
            out.push(c);
        }
    }
    out
}

/// Compare a recorded sentence with a span.
#[must_use]
pub fn drift(sentence: &str, span: &Span) -> Drift {
    if span.inline_code {
        Drift::NotCheckable
    } else if normalize(sentence) == normalize(&span.text) {
        Drift::Same
    } else {
        Drift::Drifted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization() {
        assert_eq!(normalize("  a \t\u{a0} b\n"), "a b");
        assert_eq!(normalize("e\u{301}"), "\u{e9}");
        assert_eq!(normalize(""), "");
        assert_eq!(normalize(" \u{2003} "), "");
    }
}
