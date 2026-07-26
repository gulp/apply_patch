//! Path safety integration tests.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;

fn bin() -> Command {
    Command::cargo_bin("agent-patch").unwrap()
}

#[test]
fn absolute_path_rejected() {
    let dir = tempdir().unwrap();
    let patch = "*** Begin Patch\n*** Add File: /tmp/evil\n+x\n*** End Patch\n";
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
fn symlink_escape_rejected() {
    let dir = tempdir().unwrap();
    let outside = tempdir().unwrap();
    let link = dir.path().join("link");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(outside.path(), &link).unwrap();
        fs::write(outside.path().join("secret.txt"), "secret\n").unwrap();
        let patch = "*** Begin Patch\n*** Update File: link/secret.txt\n@@\n-secret\n+pwned\n*** End Patch\n";
        bin()
            .current_dir(dir.path())
            .arg("--json")
            .write_stdin(patch)
            .assert()
            .failure()
            .code(4);
        assert_eq!(
            fs::read_to_string(outside.path().join("secret.txt")).unwrap(),
            "secret\n"
        );
    }
}

#[test]
fn windows_drive_path_rejected() {
    let dir = tempdir().unwrap();
    let patch = "*** Begin Patch\n*** Add File: C:/Windows/system.ini\n+x\n*** End Patch\n";
    bin()
        .current_dir(dir.path())
        .arg("--json")
        .write_stdin(patch)
        .assert()
        .failure()
        .code(4)
        .stdout(predicate::str::contains("INVALID_PATH"));
}
