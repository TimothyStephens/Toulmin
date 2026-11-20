//! Item 59: run one conformance case directory and write the item 92
//! `expected.json` object for it to standard output, never `expected.json`
//! itself, so the differential script and the P5 checkpoint run without
//! `tm`.
//!
//! The contract both refcheck implementations share (item 89): one case
//! directory as the only argument; the object on standard output; exit 0
//! whatever the verdict, `error` included; exit 1 with a message on standard
//! error only when the case cannot be run at all (a missing file, an
//! unreadable spec); exit 2 on a usage error. A directory holding
//! `case.json` is an `assertions/eval/` case and gives `{verdict, error}`.
//!
//! `cargo run -p tm-checker --example refcheck -- <case-dir>`

#[path = "../tests/common/case.rs"]
mod case;

use std::io::Write as _;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [arg] = args.as_slice() else {
        eprintln!("usage: refcheck <case-dir>");
        return ExitCode::from(2);
    };
    let dir = Path::new(arg);
    let got = if dir.join("case.json").is_file() {
        case::run_eval(dir)
    } else {
        case::run_case(dir).map(|(v, _)| v)
    };
    match got.and_then(|v| serde_json::to_string_pretty(&v).map_err(|e| e.to_string())) {
        Ok(text) => {
            let mut out = std::io::stdout().lock();
            if writeln!(out, "{text}").and_then(|()| out.flush()).is_err() {
                eprintln!("{arg}: cannot write standard output");
                return ExitCode::FAILURE;
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{arg}: {e}");
            ExitCode::FAILURE
        }
    }
}
