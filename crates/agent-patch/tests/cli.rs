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
    bin()
        .current_dir(dir.path())
        .arg("--json")
        .write_stdin(patch)
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::contains("HUNK_AMBIGUOUS"));
    assert_eq!(
        fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "x\ny\nx\ny\n"
    );
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
