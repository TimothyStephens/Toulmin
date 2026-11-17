//! Any bytes as a CSV file under a fixed parse spec: Files' reader never
//! panics, and a table it accepts can be read by the Checker.
#![no_main]

use libfuzzer_sys::fuzz_target;
use serde_json::json;
use tm_checker::table::Table;
use tm_formats::parse_spec::ParseSpec;

fuzz_target!(|data: &[u8]| {
    if data.len() > 4096 {
        return;
    }
    let Ok(spec) = ParseSpec::from_json(&json!({
        "spec":1,"format":"csv","delimiter":",","header":true,
        "columns":[
            {"name":"id","type":"str","missing":[]},
            {"name":"x","type":"dec","missing":["NA",""],"scale":3},
            {"name":"n","type":"int","missing":["NA"]},
            {"name":"f","type":"float64","missing":["NA"]},
            {"name":"d","type":"date","missing":["NA"]}
        ],
        "key":["id"],"nan":"value","inf":"missing","encoding":"utf-8"
    })) else {
        return;
    };
    let Ok(dir) = tempfile::tempdir() else {
        return;
    };
    let path = dir.path().join("t.csv");
    if std::fs::write(&path, data).is_err() {
        return;
    }
    let settings = tm_duckdb::EngineSettings {
        temp_directory: dir.path().to_string_lossy().into_owned(),
        ..tm_duckdb::EngineSettings::default()
    };
    match tm_files::parse::parse(&path, &spec, &settings) {
        Ok(batch) => {
            Table::from_arrow(&batch, &spec)
                .unwrap_or_else(|e| panic!("accepted but unreadable: {e}"));
        }
        Err(_) => {}
    }
});
