//! `bench-index.sh`'s runner (items 8 and 76): open a record directory,
//! rebuild the index into a file in a temporary directory, and print the
//! elapsed seconds.
//!
//!     index-rebuild --record DIR [--index FILE]

#![allow(
    clippy::expect_used,
    reason = "a command line: failures are reported by panicking"
)]

use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut record: Option<PathBuf> = None;
    let mut index: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--record" => record = Some(PathBuf::from(it.next().expect("--record DIR"))),
            "--index" => index = Some(PathBuf::from(it.next().expect("--index FILE"))),
            other => panic!("unknown argument {other}; use --record DIR [--index FILE]"),
        }
    }
    let record = record.expect("--record DIR is required");
    let tmp;
    let index = match index {
        Some(p) => p,
        None => {
            tmp = tempfile_dir();
            tmp.join("index.sqlite")
        }
    };
    let record = tm_record::Record::open(&record).expect("open the record");
    let mut idx = tm_core::index::open(&index).expect("open the index");
    let start = Instant::now();
    let folded = tm_core::index::rebuild(&mut idx, &record).expect("rebuild");
    let elapsed = start.elapsed().as_secs_f64();
    println!(
        "index-rebuild: {folded} events folded in {elapsed:.2}s into {}",
        index.display()
    );
}

fn tempfile_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tm-index-rebuild-{}",
        tm_fsfacts::racy::random_hex()
    ));
    std::fs::create_dir_all(&dir).expect("create the temporary directory");
    dir
}
