//! Item 59: run one conformance case directory and write `observed.json`
//! beside it, in the shape of `expected.json` (and of `refcheck.py`'s
//! output, P5), so the differential script and the P5 checkpoint run
//! without `tm`. Never writes `expected.json`.
//!
//! `cargo run -p tm-checker --example refcheck -- <case-dir>...`

#[path = "../tests/common/case.rs"]
mod case;

use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut status = ExitCode::SUCCESS;
    for arg in std::env::args().skip(1) {
        let dir = Path::new(&arg);
        let got = if dir.join("case.json").is_file() {
            case::run_eval(dir)
        } else {
            case::run_case(dir).map(|(v, _)| v)
        };
        match got.and_then(|v| serde_json::to_string_pretty(&v).map_err(|e| e.to_string())) {
            Ok(text) => {
                if let Err(e) = std::fs::write(dir.join("observed.json"), text + "\n") {
                    eprintln!("{arg}: {e}");
                    status = ExitCode::FAILURE;
                }
            }
            Err(e) => {
                eprintln!("{arg}: {e}");
                status = ExitCode::FAILURE;
            }
        }
    }
    status
}
