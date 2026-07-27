//! Phase 0: frozen schema fixtures must parse and declare version 2.

use serde_json::Value;
use std::fs;
use std::path::PathBuf;

fn schemas_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/schemas")
}

fn load(name: &str) -> Value {
    let path = schemas_dir().join(name);
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

#[test]
fn execution_plan_schema_is_version_2() {
    let v = load("execution-plan.schema.json");
    assert_eq!(v["properties"]["version"]["const"], 2);
    assert!(v["required"]
        .as_array()
        .unwrap()
        .iter()
        .any(|x| x.as_str() == Some("plan_digest")));
}

#[test]
fn receipt_schema_is_version_2() {
    let v = load("receipt.schema.json");
    assert_eq!(v["properties"]["version"]["const"], 2);
    let required = v["required"].as_array().unwrap();
    assert!(required
        .iter()
        .any(|x| x.as_str() == Some("transaction_id")));
    assert!(required.iter().any(|x| x.as_str() == Some("plan_digest")));
    let file_required = v["properties"]["files"]["items"]["required"]
        .as_array()
        .unwrap();
    assert!(file_required
        .iter()
        .any(|x| x.as_str() == Some("before_object")));
}

#[test]
fn journal_schema_states_cover_recovery_table() {
    let v = load("journal.schema.json");
    assert_eq!(v["properties"]["version"]["const"], 2);
    let states = v["properties"]["state"]["enum"].as_array().unwrap();
    let want = [
        "PREPARED",
        "COMMITTING",
        "COMPLETED",
        "ROLLING_BACK",
        "ROLLED_BACK",
    ];
    for s in want {
        assert!(
            states.iter().any(|x| x.as_str() == Some(s)),
            "missing journal state {s}"
        );
    }
}
