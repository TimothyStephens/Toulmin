//! Item 40's command line: `gen-chain --events N --out DIR` or
//! `gen-chain --conformance DIR`; the work is `tm_record::gen_chain`.

#![allow(
    clippy::expect_used,
    reason = "a command line: failures are reported by panicking"
)]

use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut events = 1_000_000u64;
    let mut out: Option<PathBuf> = None;
    let mut conf: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--events" => events = it.next().expect("--events N").parse().expect("a number"),
            "--out" => out = Some(PathBuf::from(it.next().expect("--out DIR"))),
            "--conformance" => conf = Some(PathBuf::from(it.next().expect("--conformance DIR"))),
            other => {
                panic!("unknown argument {other}; use --events N --out DIR or --conformance DIR")
            }
        }
    }
    match (out, conf) {
        (Some(out), None) => tm_record::gen_chain::bench(events, &out),
        (None, Some(root)) => tm_record::gen_chain::conformance(&root),
        _ => panic!("use --events N --out DIR, or --conformance DIR"),
    }
}
