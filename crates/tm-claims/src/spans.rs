//! The 11.22 span scanner (item 68; D10). A span is `[` text `]{`
//! attributes `}` on one logical line, the first attribute `.claim` and
//! one `ref=<ref>`. Spans inside fenced code blocks are not spans; a span
//! records the class of the nearest heading line above it.
//!
//! The scanner is pure: the Core reads the report from the working tree
//! and writes the `spans` table (11.17: scan-derived, never folded).

use tm_formats::id::ClaimRef;

/// One span as the scanner found it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    /// The ref it points at.
    pub claim_ref: ClaimRef,
    /// The logical line, from 1.
    pub line: u64,
    /// The text, each `\]` replaced by `]`, otherwise as written.
    pub text: String,
    /// The text holds a backtick (inline code): drift is not checkable.
    pub inline_code: bool,
    /// The class of the nearest heading line above, if it has one.
    pub section_kind: Option<String>,
    /// The attributes other than `.claim` and `ref=`, as given.
    pub attributes: Vec<String>,
}

/// The rest of `line` after at most three leading spaces, or `None` when
/// a fourth space follows them.
fn after_indent(line: &str) -> Option<&str> {
    let spaces = line.bytes().take_while(|b| *b == b' ').count();
    (spaces <= 3).then(|| &line[spaces..])
}

/// A fence opener: the fence character and its run length.
fn fence_open(line: &str) -> Option<(u8, usize)> {
    let rest = after_indent(line)?;
    let c = *rest.as_bytes().first()?;
    if c != b'`' && c != b'~' {
        return None;
    }
    let run = rest.bytes().take_while(|b| *b == c).count();
    (run >= 3).then_some((c, run))
}

fn fence_closes(line: &str, c: u8, len: usize) -> bool {
    let Some(rest) = after_indent(line) else {
        return false;
    };
    let run = rest.bytes().take_while(|b| *b == c).count();
    run >= len && rest[run..].chars().all(char::is_whitespace)
}

/// An ATX heading line and its class, if it has one.
#[derive(Debug, PartialEq, Eq)]
struct Heading {
    class: Option<String>,
}

/// The heading, for an ATX heading line; `None` for any other line.
fn heading(line: &str) -> Option<Heading> {
    let rest = after_indent(line)?;
    let hashes = rest.bytes().take_while(|b| *b == b'#').count();
    if !(1..=6).contains(&hashes) {
        return None;
    }
    match rest.as_bytes().get(hashes) {
        None | Some(b' ' | b'\t') => {}
        Some(_) => return None,
    }
    let trimmed = line.trim_end();
    let class = trimmed
        .strip_suffix('}')
        .and_then(|s| s.rfind('{').map(|i| &s[i + 1..]))
        .and_then(|s| s.strip_prefix('.'))
        .filter(|k| !k.is_empty() && !k.chars().any(|c| c.is_whitespace() || c == '{' || c == '}'))
        .map(str::to_owned);
    Some(Heading { class })
}

/// The ref and the other attributes, when the attributes make a span.
fn attributes(text: &str) -> Option<(ClaimRef, Vec<String>)> {
    let mut parts = text.split_whitespace();
    if parts.next()? != ".claim" {
        return None;
    }
    let mut claim_ref = None;
    let mut others = Vec::new();
    for p in parts {
        if let Some(r) = p.strip_prefix("ref=") {
            if claim_ref.is_some() {
                return None;
            }
            claim_ref = Some(ClaimRef::parse(r)?);
        } else {
            others.push(p.to_owned());
        }
    }
    Some((claim_ref?, others))
}

/// The spans of one line, left to right.
fn line_spans(line: &str, number: u64, section_kind: Option<&String>, out: &mut Vec<Span>) {
    let b = line.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'[' {
            i += 1;
            continue;
        }
        let close = (i + 1..b.len()).find(|&k| b[k] == b']' && b[k - 1] != b'\\');
        let found = close.and_then(|k| {
            if b.get(k + 1) != Some(&b'{') {
                return None;
            }
            let end = (k + 2..b.len()).find(|&m| b[m] == b'}')?;
            let (claim_ref, attrs) = attributes(&line[k + 2..end])?;
            Some((k, end, claim_ref, attrs))
        });
        match found {
            Some((k, end, claim_ref, attributes)) => {
                let text = line[i + 1..k].replace("\\]", "]");
                out.push(Span {
                    claim_ref,
                    line: number,
                    inline_code: text.contains('`'),
                    text,
                    section_kind: section_kind.cloned(),
                    attributes,
                });
                i = end + 1;
            }
            None => i += 1,
        }
    }
}

/// Every span of a report, in file order: by line, then by position.
#[must_use]
pub fn scan(report: &str) -> Vec<Span> {
    let mut out = Vec::new();
    let mut fence: Option<(u8, usize)> = None;
    let mut section_kind: Option<String> = None;
    for (n, raw) in report.split('\n').enumerate() {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if let Some((c, len)) = fence {
            if fence_closes(line, c, len) {
                fence = None;
            }
            continue;
        }
        if let Some(open) = fence_open(line) {
            fence = Some(open);
            continue;
        }
        let number = u64::try_from(n).unwrap_or(u64::MAX - 1) + 1;
        line_spans(line, number, section_kind.as_ref(), &mut out);
        if let Some(h) = heading(line) {
            section_kind = h.class;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules() {
        let s = scan("[a [b]{.claim ref=C1}");
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].text, "a [b");
        assert_eq!(scan("[x]{.claim ref=C01}").len(), 0);
        assert_eq!(scan("[x]{.claim ref=C1 ref=C2}").len(), 0);
        assert_eq!(scan("[x]{ref=C1 .claim}").len(), 0);
        assert_eq!(scan("[x] {.claim ref=C1}").len(), 0);
        assert_eq!(
            scan("[x]{.claim ref=S4.12}")[0].claim_ref,
            ClaimRef::Session(4, 12)
        );
        let two = scan("[a]{.claim ref=C1} and [b]{.claim ref=C1}");
        assert_eq!(two.len(), 2);
        assert_eq!(heading("####### x"), None);
        assert_eq!(heading("#x"), None);
        assert_eq!(heading("    # x"), None);
        assert_eq!(
            heading("# x {.results}  "),
            Some(Heading {
                class: Some("results".into())
            })
        );
        assert_eq!(heading("# x {.}"), Some(Heading { class: None }));
        assert_eq!(heading("#"), Some(Heading { class: None }));
        assert_eq!(fence_open("   ```rust"), Some((b'`', 3)));
        assert_eq!(fence_open("    ```"), None);
        assert!(!fence_closes("```` x", b'`', 3));
        assert!(!fence_closes("``", b'`', 3));
    }
}
