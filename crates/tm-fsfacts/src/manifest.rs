//! Directory manifests (item 28; 11.16's `manifest` object): entries sorted
//! in 11.10 order, each with its snapshot and fingerprint; exclude
//! patterns with the `manifest.ignore` default; and the diff between two
//! manifests.
//!
//! The manifest fingerprint is not computed here: decision 10's rule
//! (`sha256:` of the canonical JSON of `entries`, snapshots included)
//! collides with 5.6's content labels and D22, and the P2 report raises it
//! as a collision for the owner to rule on. `entries_json` is what either
//! rule would digest.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tm_formats::sort_order::{cmp_text, nfc};
use tm_formats::{Code, Error as FormatError};

use crate::Error;
use crate::hash::{Fingerprint, HashSettings, hash_files};
use crate::snapshot::Snapshot;

/// The `manifest.ignore` default (11.19).
pub const DEFAULT_IGNORE: &[&str] = &[
    ".snakemake/",
    ".nextflow/",
    "__pycache__/",
    "*.swp",
    ".DS_Store",
];

/// One file in a directory version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Relative to the directory, `/`-separated, NFC.
    pub path: String,
    /// Its stat facts.
    pub snapshot: Snapshot,
    /// Its content fingerprint.
    pub fingerprint: Fingerprint,
}

impl Entry {
    /// The 11.16 entry object.
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "path": self.path,
            "snapshot": self.snapshot.to_json(),
            "fingerprint": self.fingerprint.as_str(),
        })
    }
}

/// A directory's entries and the exclude list that produced them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    /// The exclude patterns applied, as given.
    pub ignore: Vec<String>,
    /// Entries sorted by `path` in 11.10 order.
    pub entries: Vec<Entry>,
}

/// What changed between two manifests, by entry path.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ManifestDiff {
    /// In the new manifest only.
    pub added: Vec<String>,
    /// In the old manifest only.
    pub removed: Vec<String>,
    /// In both with a different fingerprint.
    pub modified: Vec<String>,
}

impl ManifestDiff {
    /// True when nothing changed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.modified.is_empty()
    }
}

impl Manifest {
    /// Walk `dir`, snapshot and hash every entry not excluded.
    ///
    /// # Errors
    /// `E3003` for a non-UTF-8 path inside the directory; `Error::Io` for
    /// anything the operating system refuses; an invalid pattern as
    /// `Error::Io` at the pattern text.
    pub fn build(
        dir: &Path,
        ignore: &[String],
        settings: &HashSettings,
    ) -> Result<Manifest, Error> {
        let excludes = Excludes::parse(ignore)?;
        let mut found: Vec<(String, PathBuf)> = Vec::new();
        walk(dir, dir, &excludes, &mut found)?;
        found.sort_by(|a, b| cmp_text(&a.0, &b.0));
        let paths: Vec<PathBuf> = found.iter().map(|(_, p)| p.clone()).collect();
        let hashed = hash_files(&paths, settings);
        let mut entries = Vec::with_capacity(found.len());
        for ((rel, full), h) in found.into_iter().zip(hashed) {
            let snapshot = Snapshot::take(&full)?;
            entries.push(Entry {
                path: rel,
                snapshot,
                fingerprint: h?.fingerprint,
            });
        }
        Ok(Manifest {
            ignore: ignore.to_vec(),
            entries,
        })
    }

    /// The `entries` list as JSON, in order.
    #[must_use]
    pub fn entries_json(&self) -> Value {
        Value::Array(self.entries.iter().map(Entry::to_json).collect())
    }

    /// A directory version's `size`: the sum of its entries' sizes (item 28).
    #[must_use]
    pub fn size(&self) -> u64 {
        self.entries.iter().map(|e| e.snapshot.size).sum()
    }

    /// Entries added, removed, and modified from `old` to `self`.
    #[must_use]
    pub fn diff(&self, old: &Manifest) -> ManifestDiff {
        let olds: BTreeMap<&str, &Fingerprint> = old
            .entries
            .iter()
            .map(|e| (e.path.as_str(), &e.fingerprint))
            .collect();
        let news: BTreeMap<&str, &Fingerprint> = self
            .entries
            .iter()
            .map(|e| (e.path.as_str(), &e.fingerprint))
            .collect();
        let mut d = ManifestDiff::default();
        for e in &self.entries {
            match olds.get(e.path.as_str()) {
                None => d.added.push(e.path.clone()),
                Some(fp) if **fp != e.fingerprint => d.modified.push(e.path.clone()),
                Some(_) => {}
            }
        }
        for e in &old.entries {
            if !news.contains_key(e.path.as_str()) {
                d.removed.push(e.path.clone());
            }
        }
        d
    }
}

fn walk(
    root: &Path,
    dir: &Path,
    excludes: &Excludes,
    out: &mut Vec<(String, PathBuf)>,
) -> Result<(), Error> {
    let rd = std::fs::read_dir(dir).map_err(|e| Error::io(dir, e))?;
    for entry in rd {
        let entry = entry.map_err(|e| Error::io(dir, e))?;
        let full = entry.path();
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            return Err(FormatError::new(
                Code::E3003,
                format!("path is not UTF-8: {}", full.display()),
            )
            .into());
        };
        let name = nfc(name);
        let rel = match full.parent().and_then(|p| p.strip_prefix(root).ok()) {
            Some(p) if p.as_os_str().is_empty() => name.clone(),
            Some(p) => {
                let Some(parent) = p.to_str() else {
                    return Err(FormatError::new(
                        Code::E3003,
                        format!("path is not UTF-8: {}", full.display()),
                    )
                    .into());
                };
                format!("{}/{name}", nfc(parent))
            }
            None => name.clone(),
        };
        // Links, sockets, pipes, and devices are not entries; a link to a
        // directory is not walked, so a cycle cannot occur.
        let ft = entry.file_type().map_err(|e| Error::io(&full, e))?;
        if ft.is_dir() {
            if !excludes.excluded_dir(&rel) {
                walk(root, &full, excludes, out)?;
            }
        } else if ft.is_file() && !excludes.excluded_file(&rel) {
            out.push((rel, full));
        }
    }
    Ok(())
}

/// A parsed exclude list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Excludes {
    patterns: Vec<Pattern>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Pattern {
    /// `name/`: a directory by its final component, anywhere.
    Dir(Vec<Tok>),
    /// No `/`: an entry's final component.
    Name(Vec<Tok>),
    /// A `/` elsewhere: the whole relative path.
    Path(Vec<Tok>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    Lit(char),
    Any,
    One,
    Class {
        negated: bool,
        items: Vec<(char, char)>,
    },
}

impl Excludes {
    /// Parse patterns per item 28.
    ///
    /// # Errors
    /// `Error::Io` at the pattern text for a pattern that does not parse
    /// (an unterminated class or a trailing `\`).
    pub fn parse(patterns: &[String]) -> Result<Excludes, Error> {
        let mut out = Vec::with_capacity(patterns.len());
        for p in patterns {
            let bad = |why: &str| {
                Error::io(
                    p.as_str(),
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!("not a glob: {why}"),
                    ),
                )
            };
            if p.is_empty() {
                return Err(bad("empty pattern"));
            }
            if let Some(dir) = p.strip_suffix('/') {
                if dir.contains('/') || dir.is_empty() {
                    return Err(bad("a directory pattern is one component followed by `/`"));
                }
                out.push(Pattern::Dir(
                    tokens(dir).ok_or_else(|| bad("bad class or escape"))?,
                ));
            } else if p.contains('/') {
                out.push(Pattern::Path(
                    tokens(p).ok_or_else(|| bad("bad class or escape"))?,
                ));
            } else {
                out.push(Pattern::Name(
                    tokens(p).ok_or_else(|| bad("bad class or escape"))?,
                ));
            }
        }
        Ok(Excludes { patterns: out })
    }

    /// True when a directory at `rel` is excluded (and nothing under it is
    /// walked).
    #[must_use]
    pub fn excluded_dir(&self, rel: &str) -> bool {
        let last = rel.rsplit('/').next().unwrap_or(rel);
        self.patterns.iter().any(|p| match p {
            Pattern::Dir(t) => matches(t, last),
            Pattern::Name(_) | Pattern::Path(_) => false,
        })
    }

    /// True when a file at `rel` is excluded, by its own name or path or by
    /// an excluded directory above it.
    #[must_use]
    pub fn excluded_file(&self, rel: &str) -> bool {
        let last = rel.rsplit('/').next().unwrap_or(rel);
        self.patterns.iter().any(|p| match p {
            Pattern::Dir(t) => rel.split('/').rev().skip(1).any(|c| matches(t, c)),
            Pattern::Name(t) => matches(t, last),
            Pattern::Path(t) => matches(t, rel),
        })
    }
}

fn tokens(p: &str) -> Option<Vec<Tok>> {
    let mut out = Vec::new();
    let mut it = p.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '*' => out.push(Tok::Any),
            '?' => out.push(Tok::One),
            '\\' => out.push(Tok::Lit(it.next()?)),
            '[' => {
                let negated = matches!(it.peek(), Some('!' | '^'));
                if negated {
                    it.next();
                }
                let mut items = Vec::new();
                let mut first = true;
                loop {
                    let a = it.next()?;
                    if a == ']' && !first {
                        break;
                    }
                    first = false;
                    let a = if a == '\\' { it.next()? } else { a };
                    if it.peek() == Some(&'-') {
                        it.next();
                        let b = it.next()?;
                        if b == ']' {
                            items.push((a, a));
                            items.push(('-', '-'));
                            break;
                        }
                        let b = if b == '\\' { it.next()? } else { b };
                        items.push((a, b));
                    } else {
                        items.push((a, a));
                    }
                }
                out.push(Tok::Class { negated, items });
            }
            c => out.push(Tok::Lit(c)),
        }
    }
    Some(out)
}

fn matches(t: &[Tok], s: &str) -> bool {
    fn go(t: &[Tok], s: &[char]) -> bool {
        match t.split_first() {
            None => s.is_empty(),
            Some((Tok::Any, rest)) => {
                (0..=s.len()).any(|i| s[..i].iter().all(|c| *c != '/') && go(rest, &s[i..]))
            }
            Some((tok, rest)) => {
                let Some((c, tail)) = s.split_first() else {
                    return false;
                };
                let ok = match tok {
                    Tok::Lit(l) => l == c,
                    Tok::One => *c != '/',
                    Tok::Class { negated, items } => {
                        *c != '/' && items.iter().any(|(a, b)| a <= c && c <= b) != *negated
                    }
                    Tok::Any => unreachable!(),
                };
                ok && go(rest, tail)
            }
        }
    }
    let chars: Vec<char> = s.chars().collect();
    go(t, &chars)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ex(p: &[&str]) -> Excludes {
        Excludes::parse(&p.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>()).unwrap()
    }

    #[test]
    fn default_patterns() {
        let e = ex(DEFAULT_IGNORE);
        assert!(e.excluded_dir(".snakemake"));
        assert!(e.excluded_dir("deep/er/.nextflow"));
        assert!(!e.excluded_dir("results"));
        assert!(e.excluded_file("a/b.swp"));
        assert!(e.excluded_file(".DS_Store"));
        assert!(e.excluded_file("x/.DS_Store"));
        assert!(e.excluded_file("__pycache__/m.pyc"));
        assert!(!e.excluded_file("a/b.csv"));
        assert!(!e.excluded_file("swp"));
    }

    #[test]
    fn globs() {
        let e = ex(&["results/tmp/*", "[!a]?.txt", "lit\\*", "[d-f]*"]);
        assert!(e.excluded_file("results/tmp/x.csv"));
        assert!(!e.excluded_file("results/tmp/deeper/x.csv"));
        assert!(e.excluded_file("bz.txt"));
        assert!(!e.excluded_file("az.txt"));
        assert!(e.excluded_file("lit*"));
        assert!(!e.excluded_file("litx"));
        assert!(e.excluded_file("dir/eel"));
        assert!(!e.excluded_file("dir/x"));
        assert!(Excludes::parse(&["[abc".to_owned()]).is_err());
        assert!(Excludes::parse(&["a/b/".to_owned()]).is_err());
        assert!(Excludes::parse(&["trail\\".to_owned()]).is_err());
        assert!(Excludes::parse(&[String::new()]).is_err());
    }
}
