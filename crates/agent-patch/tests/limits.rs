//! Resource limit tests.

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::tempdir;

fn bin() -> Command {
    Command::cargo_bin("agent-patch").unwrap()
}

#[test]
fn patch_bytes_limit() {
    let dir = tempdir().unwrap();
    let mut patch = String::from("*** Begin Patch\n*** Add File: a.txt\n");
    for _ in 0..200 {
        patch.push_str("+xxxxxxxx\n");
    }
    patch.push_str("*** End Patch\n");
    bin()
        .current_dir(dir.path())
        .args(["--json", "--max-patch-bytes", "64"])
        .write_stdin(patch)
        .assert()
        .failure()
        .code(7)
        .stdout(predicate::str::contains("LIMIT_PATCH_BYTES"));
}
