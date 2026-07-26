//! Atomicity: failed second op must not mutate.

use assert_cmd::Command;
use std::fs;
use tempfile::tempdir;

#[test]
fn no_partial_apply_on_validation_failure() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("ok.txt"), "one\n").unwrap();
    fs::write(dir.path().join("dup.txt"), "keep\n").unwrap();
    let patch = r#"*** Begin Patch
*** Update File: ok.txt
@@
-one
+two
*** Add File: dup.txt
+new
*** End Patch
"#;
    Command::cargo_bin("agent-patch")
        .unwrap()
        .current_dir(dir.path())
        .write_stdin(patch)
        .assert()
        .failure();
    assert_eq!(
        fs::read_to_string(dir.path().join("ok.txt")).unwrap(),
        "one\n"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("dup.txt")).unwrap(),
        "keep\n"
    );
}
