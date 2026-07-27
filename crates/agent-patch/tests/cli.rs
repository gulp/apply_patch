//! CLI integration tests.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;

fn bin() -> Command {
    Command::cargo_bin("agent-patch").unwrap()
}

#[test]
fn apply_update_from_stdin() {
    let dir = tempdir().unwrap();
    let src = dir.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("config.rs"),
        "pub const RETRIES: usize = 2;\npub const TIMEOUT_SECS: u64 = 30;\n",
    )
    .unwrap();

    let patch = r#"*** Begin Patch
*** Update File: src/config.rs
@@
 pub const RETRIES: usize = 2;
-pub const TIMEOUT_SECS: u64 = 30;
+pub const TIMEOUT_SECS: u64 = 45;
*** End Patch
"#;

    bin()
        .current_dir(dir.path())
        .write_stdin(patch)
        .assert()
        .success()
        .stdout(predicate::str::contains("apply ok"));

    let content = fs::read_to_string(src.join("config.rs")).unwrap();
    assert!(content.contains("TIMEOUT_SECS: u64 = 45"));
    assert!(!content.contains("= 30;"));
}

#[test]
fn check_does_not_write() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("a.txt"), "hello\n").unwrap();
    let patch = r#"*** Begin Patch
*** Update File: a.txt
@@
-hello
+world
*** End Patch
"#;
    bin()
        .current_dir(dir.path())
        .arg("--check")
        .write_stdin(patch)
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "hello\n"
    );
}

#[test]
fn json_mode_success() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("a.txt"), "hello\n").unwrap();
    let patch = r#"*** Begin Patch
*** Update File: a.txt
@@
-hello
+world
*** End Patch
"#;
    bin()
        .current_dir(dir.path())
        .args(["--json", "--check"])
        .write_stdin(patch)
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""ok": true"#));
}

#[test]
fn rejects_traversal() {
    let dir = tempdir().unwrap();
    let patch = r#"*** Begin Patch
*** Add File: ../outside.txt
+nope
*** End Patch
"#;
    bin()
        .current_dir(dir.path())
        .arg("--json")
        .write_stdin(patch)
        .assert()
        .failure()
        .code(4)
        .stdout(predicate::str::contains("INVALID_PATH"));
}

#[test]
fn ambiguous_hunk() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("a.txt"), "x\ny\nx\ny\n").unwrap();
    let patch = r#"*** Begin Patch
*** Update File: a.txt
@@
-x
+z
*** End Patch
"#;
    let assert = bin()
        .current_dir(dir.path())
        .arg("--json")
        .write_stdin(patch)
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::contains("HUNK_AMBIGUOUS"))
        .stdout(predicate::str::contains("candidates"));
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let cands = v["error"]["candidates"].as_array().unwrap();
    assert!(cands.len() >= 2, "oracle should list ≥2 candidates");
    assert_eq!(
        fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "x\ny\nx\ny\n"
    );
}

#[test]
fn plan_mode_zero_writes_and_digest() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("a.txt"), "hello\n").unwrap();
    let patch = r#"*** Begin Patch
*** Update File: a.txt
@@
-hello
+world
*** End Patch
"#;
    let assert = bin()
        .current_dir(dir.path())
        .args(["--plan", "--json"])
        .write_stdin(patch)
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(v["mode"], "plan");
    assert_eq!(v["version"], 2);
    assert!(v["plan_digest"].as_str().unwrap().starts_with("blake3:"));
    assert_eq!(v["plan"]["version"], 2);
    assert!(!v["plan"]["entries"].as_array().unwrap().is_empty());
    assert_eq!(
        fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "hello\n"
    );
}

#[test]
fn plan_and_check_are_exclusive() {
    bin()
        .args(["--plan", "--check"])
        .write_stdin("*** Begin Patch\n*** End Patch\n")
        .assert()
        .failure()
        .code(2);
}

#[test]
fn add_update_delete() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("keep.txt"), "old\n").unwrap();
    fs::write(dir.path().join("gone.txt"), "bye\n").unwrap();
    let patch = r#"*** Begin Patch
*** Add File: new.txt
+fresh
*** Update File: keep.txt
@@
-old
+new
*** Delete File: gone.txt
*** End Patch
"#;
    bin()
        .current_dir(dir.path())
        .write_stdin(patch)
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(dir.path().join("new.txt")).unwrap(),
        "fresh\n"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("keep.txt")).unwrap(),
        "new\n"
    );
    assert!(!dir.path().join("gone.txt").exists());
}

#[test]
fn malformed_patch_exit_2() {
    bin()
        .write_stdin("not a patch\n")
        .assert()
        .failure()
        .code(2);
}

#[test]
fn second_op_failure_leaves_tree_unchanged() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("a.txt"), "a\n").unwrap();
    let patch = r#"*** Begin Patch
*** Update File: a.txt
@@
-a
+b
*** Update File: missing.txt
@@
-x
+y
*** End Patch
"#;
    bin()
        .current_dir(dir.path())
        .write_stdin(patch)
        .assert()
        .failure();
    assert_eq!(fs::read_to_string(dir.path().join("a.txt")).unwrap(), "a\n");
}

#[test]
fn apply_writes_completed_journal_and_status_ok() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("a.txt"), "hello\n").unwrap();
    let patch = r#"*** Begin Patch
*** Update File: a.txt
@@
-hello
+world
*** End Patch
"#;
    let out = bin()
        .current_dir(dir.path())
        .args(["--json"])
        .write_stdin(patch)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    let txid = v["transaction_id"].as_str().expect("transaction_id");
    let journal = dir
        .path()
        .join(".agent-patch/transactions")
        .join(txid)
        .join("journal.json");
    assert!(journal.is_file());
    let j: serde_json::Value = serde_json::from_slice(&fs::read(&journal).unwrap()).unwrap();
    assert_eq!(j["state"], "COMPLETED");

    bin()
        .current_dir(dir.path())
        .args(["status", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"ok\": true"));
}

#[test]
fn recover_clears_prepared_incomplete() {
    let dir = tempdir().unwrap();
    // Seed a PREPARED journal as if a crash happened before mutate.
    let tx = dir.path().join(".agent-patch/transactions/deadbeef");
    fs::create_dir_all(&tx).unwrap();
    fs::create_dir_all(dir.path().join(".agent-patch/objects")).unwrap();
    fs::create_dir_all(dir.path().join(".agent-patch/receipts")).unwrap();
    fs::write(
        tx.join("journal.json"),
        r#"{
  "version": 2,
  "transaction_id": "deadbeef",
  "plan_digest": "blake3:x",
  "state": "PREPARED",
  "created_at": "0",
  "entries": []
}"#,
    )
    .unwrap();

    bin()
        .current_dir(dir.path())
        .args(["status", "--json"])
        .assert()
        .failure()
        .code(1);

    bin()
        .current_dir(dir.path())
        .args(["recover", "--transaction", "deadbeef", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("rolled_back"));

    bin()
        .current_dir(dir.path())
        .args(["status", "--json"])
        .assert()
        .success();
}

#[test]
fn apply_receipt_export_and_revert() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("a.txt"), "hello\n").unwrap();
    let receipt_path = dir.path().join("out-receipt.json");
    let patch = r#"*** Begin Patch
*** Update File: a.txt
@@
-hello
+world
*** End Patch
"#;
    bin()
        .current_dir(dir.path())
        .args(["--receipt", receipt_path.to_str().unwrap()])
        .write_stdin(patch)
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "world\n"
    );
    assert!(receipt_path.is_file());

    bin()
        .current_dir(dir.path())
        .args(["revert", receipt_path.to_str().unwrap()])
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "hello\n"
    );

    // Stale revert fails closed
    bin()
        .current_dir(dir.path())
        .args(["revert", receipt_path.to_str().unwrap()])
        .assert()
        .failure()
        .code(5);
}

#[test]
fn verify_promotes_on_success_and_skips_on_failure() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("a.txt"), "hello\n").unwrap();
    let patch = r#"*** Begin Patch
*** Update File: a.txt
@@
-hello
+world
*** End Patch
"#;

    bin()
        .current_dir(dir.path())
        .args(["--verify", "--", "false"])
        .write_stdin(patch)
        .assert()
        .failure()
        .code(1);
    assert_eq!(
        fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "hello\n"
    );

    bin()
        .current_dir(dir.path())
        .args(["--verify", "--", "true"])
        .write_stdin(patch)
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "world\n"
    );
}

#[test]
fn fuzzy_rstrip_unique_only() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("a.txt"), "hello  \n").unwrap();
    let patch = r#"*** Begin Patch
*** Update File: a.txt
@@
-hello
+world
*** End Patch
"#;
    bin()
        .current_dir(dir.path())
        .args(["--fuzzy", "off"])
        .write_stdin(patch)
        .assert()
        .failure();
    bin()
        .current_dir(dir.path())
        .args(["--fuzzy", "rstrip"])
        .write_stdin(patch)
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "world\n"
    );
}

#[test]
fn hash_pin_mismatch_fails_before_apply() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("a.txt"), "hello\n").unwrap();
    let patch = r#"*** Begin Patch
*** Update File: a.txt
*** Hash: blake3 0000000000000000000000000000000000000000000000000000000000000000
@@
-hello
+world
*** End Patch
"#;
    bin()
        .current_dir(dir.path())
        .write_stdin(patch)
        .assert()
        .failure()
        .code(5);
    assert_eq!(
        fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "hello\n"
    );
}

#[test]
fn idempotent_replay_succeeds() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("a.txt"), "hello\n").unwrap();
    let patch = r#"*** Begin Patch
*** Update File: a.txt
@@
-hello
+world
*** End Patch
"#;
    bin()
        .current_dir(dir.path())
        .write_stdin(patch)
        .assert()
        .success();
    let out = bin()
        .current_dir(dir.path())
        .args(["--idempotent", "--json"])
        .write_stdin(patch)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["already_applied"], true);
    assert_eq!(
        fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "world\n"
    );
}

#[test]
fn verify_shell_promotes_on_success() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("a.txt"), "hello\n").unwrap();
    let patch = r#"*** Begin Patch
*** Update File: a.txt
@@
-hello
+world
*** End Patch
"#;
    bin()
        .current_dir(dir.path())
        .args(["--verify-shell", "test -f a.txt"])
        .write_stdin(patch)
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "world\n"
    );
}

#[test]
fn event_log_records_apply() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("a.txt"), "hello\n").unwrap();
    let log = dir.path().join("events.jsonl");
    let patch = r#"*** Begin Patch
*** Update File: a.txt
@@
-hello
+world
*** End Patch
"#;
    bin()
        .current_dir(dir.path())
        .env("AGENT_PATCH_EVENT_LOG", &log)
        .args(["--json"])
        .write_stdin(patch)
        .assert()
        .success();
    let body = fs::read_to_string(&log).unwrap();
    assert!(body.contains("\"phase\":\"apply\""));
    assert!(body.contains("\"ok\":true"));
}

#[cfg(unix)]
#[test]
fn revert_restores_mode_bits() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempdir().unwrap();
    let path = dir.path().join("script.sh");
    fs::write(&path, "#!/bin/sh\necho hi\n").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o750)).unwrap();
    let receipt = dir.path().join("r.json");
    let patch = r#"*** Begin Patch
*** Update File: script.sh
@@
-#!/bin/sh
-echo hi
+#!/bin/sh
+echo hello
*** End Patch
"#;
    bin()
        .current_dir(dir.path())
        .args(["--receipt", receipt.to_str().unwrap()])
        .write_stdin(patch)
        .assert()
        .success();
    bin()
        .current_dir(dir.path())
        .args(["revert", receipt.to_str().unwrap()])
        .assert()
        .success();
    let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o750);
    assert_eq!(fs::read_to_string(&path).unwrap(), "#!/bin/sh\necho hi\n");
}

#[test]
fn ambiguous_includes_repair_patch() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("f.txt"), "x\ny\nx\ny\n").unwrap();
    let patch = r#"*** Begin Patch
*** Update File: f.txt
@@
-x
+z
*** End Patch
"#;
    let assert = bin()
        .current_dir(dir.path())
        .arg("--json")
        .write_stdin(patch)
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::contains("HUNK_AMBIGUOUS"))
        .stdout(predicate::str::contains("repair_patch"));
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let repair = v["error"]["repair_patch"].as_str().unwrap();
    assert!(repair.contains("*** Begin Patch"));
    assert!(repair.contains("*** Update File: f.txt"));
}

#[test]
fn risk_refuse_blocks_fuzzy_accept() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("f.txt"), "hello world\n").unwrap();
    let patch = r#"*** Begin Patch
*** Update File: f.txt
@@
-hello world 
+hello there
*** End Patch
"#;
    bin()
        .current_dir(dir.path())
        .args(["--json", "--fuzzy=rstrip", "--risk=refuse"])
        .write_stdin(patch)
        .assert()
        .failure()
        .stdout(predicate::str::contains("RISK_REFUSED"));
}

#[test]
fn risk_warn_surfaces_fuzzy_findings() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("f.txt"), "hello world\n").unwrap();
    let patch = r#"*** Begin Patch
*** Update File: f.txt
@@
-hello world 
+hello there
*** End Patch
"#;
    let out = bin()
        .current_dir(dir.path())
        .args(["--json", "--fuzzy=rstrip", "--risk=warn"])
        .write_stdin(patch)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    let warnings = v["warnings"].as_array().unwrap();
    assert!(!warnings.is_empty(), "{v}");
    assert!(
        warnings
            .iter()
            .any(|w| w.as_str().unwrap().contains("Rstrip")),
        "{v}"
    );
}

#[test]
fn verify_timeout_flag_trips() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("a.txt"), "a\n").unwrap();
    let patch = dir.path().join("p.patch");
    fs::write(
        &patch,
        "*** Begin Patch\n*** Update File: a.txt\n@@\n-a\n+A\n*** End Patch\n",
    )
    .unwrap();
    let out = bin()
        .current_dir(dir.path())
        .args([
            "--json",
            "--verify-timeout",
            "1",
            "--verify",
            patch.to_str().unwrap(),
            "--",
            "sleep",
            "30",
        ])
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(v["error"]["code"], "VERIFY_TIMEOUT");
    assert_eq!(fs::read_to_string(dir.path().join("a.txt")).unwrap(), "a\n");
}
