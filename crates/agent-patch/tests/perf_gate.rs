//! Light CI performance gate (generous budget; not a microbenchmark).

use assert_cmd::Command;
use std::fs;
use std::time::Instant;
use tempfile::tempdir;

#[test]
fn perf_gate_ten_file_apply_under_budget() {
    let dir = tempdir().unwrap();
    let mut patch = String::from("*** Begin Patch\n");
    for i in 0..10 {
        let name = format!("f{i}.txt");
        fs::write(dir.path().join(&name), format!("line-{i}-old\n")).unwrap();
        patch.push_str(&format!(
            "*** Update File: {name}\n@@\n-line-{i}-old\n+line-{i}-new\n"
        ));
    }
    patch.push_str("*** End Patch\n");

    let started = Instant::now();
    Command::cargo_bin("agent-patch")
        .unwrap()
        .current_dir(dir.path())
        .arg("--quiet")
        .write_stdin(patch)
        .assert()
        .success();
    let elapsed = started.elapsed();
    // Generous CI budget (local targets in the plan are much lower).
    assert!(
        elapsed.as_secs_f64() < 5.0,
        "10-file apply took {elapsed:?} (budget 5s)"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("f0.txt")).unwrap(),
        "line-0-new\n"
    );
}
