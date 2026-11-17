//! Item 61: the generated SQL of every conformance case as an `insta`
//! snapshot, so a translator change is visible in review. Regression only:
//! correctness is `tests/conformance.rs`'s.

mod common;

use common::case;

#[test]
fn generated_sql_per_case() {
    let mut settings = insta::Settings::clone_current();
    settings.set_prepend_module_to_snapshot(false);
    settings.set_omit_expression(true);
    settings.bind(|| {
        for dir in case::case_dirs(&case::checker_root().join("cases")) {
            let name = dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?")
                .to_owned();
            let sql = match case::run_case(&dir) {
                Ok((_, r)) => r.verdict.sql,
                Err(e) => format!("harness error: {e}"),
            };
            insta::assert_snapshot!(name, sql);
        }
    });
}
