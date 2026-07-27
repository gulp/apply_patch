//! Killpoint / recover crash matrix (requires `--features failpoints`).
//!
//! Spawns `agent-patch` with `AGENT_PATCH_FAILPOINT=<name>`, expects abort,
//! then `recover` must restore exact all-before or finalize all-after.

#![cfg(feature = "failpoints")]

use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::tempdir;

fn bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_agent-patch"))
}

fn write_tree(root: &Path) {
    fs::write(root.join("a.txt"), "alpha\n").unwrap();
    fs::write(root.join("b.txt"), "bravo\n").unwrap();
}

fn multi_patch() -> &'static str {
    r#"*** Begin Patch
*** Update File: a.txt
@@
-alpha
+ALPHA
*** Update File: b.txt
@@
-bravo
+BRAVO
*** End Patch
"#
}

fn single_patch() -> &'static str {
    r#"*** Begin Patch
*** Update File: a.txt
@@
-alpha
+ALPHA
*** End Patch
"#
}

fn run_apply_with_failpoint(root: &Path, patch: &str, failpoint: &str) -> std::process::ExitStatus {
    let patch_path = root.join("change.patch");
    fs::write(&patch_path, patch).unwrap();
    Command::new(bin())
        .current_dir(root)
        .env("AGENT_PATCH_FAILPOINT", failpoint)
        .arg("--json")
        .arg(&patch_path)
        .status()
        .expect("spawn apply")
}

fn recover(root: &Path) -> (i32, String) {
    let out = Command::new(bin())
        .current_dir(root)
        .args(["recover", "--json"])
        .output()
        .expect("spawn recover");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

fn status_ok(root: &Path) -> bool {
    Command::new(bin())
        .current_dir(root)
        .args(["status", "--json"])
        .status()
        .unwrap()
        .success()
}

fn assert_all_before(root: &Path) {
    assert_eq!(fs::read_to_string(root.join("a.txt")).unwrap(), "alpha\n");
    assert_eq!(fs::read_to_string(root.join("b.txt")).unwrap(), "bravo\n");
}

fn assert_all_after_multi(root: &Path) {
    assert_eq!(fs::read_to_string(root.join("a.txt")).unwrap(), "ALPHA\n");
    assert_eq!(fs::read_to_string(root.join("b.txt")).unwrap(), "BRAVO\n");
}

#[test]
fn crash_matrix_after_prepared_rolls_back() {
    let dir = tempdir().unwrap();
    write_tree(dir.path());
    let st = run_apply_with_failpoint(dir.path(), multi_patch(), "after_prepared");
    assert!(!st.success(), "failpoint should abort apply");
    assert_all_before(dir.path());
    assert!(!status_ok(dir.path()));
    let (code, out) = recover(dir.path());
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("rolled_back"), "{out}");
    assert_all_before(dir.path());
    assert!(status_ok(dir.path()));
}

#[test]
fn crash_matrix_before_visible_mutate_rolls_back() {
    let dir = tempdir().unwrap();
    write_tree(dir.path());
    let st = run_apply_with_failpoint(dir.path(), multi_patch(), "before_visible_mutate");
    assert!(!st.success());
    assert_all_before(dir.path());
    let (code, out) = recover(dir.path());
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("rolled_back"), "{out}");
    assert_all_before(dir.path());
    assert!(status_ok(dir.path()));
}

#[test]
fn crash_matrix_after_first_visible_multi_rolls_back_all_before() {
    let dir = tempdir().unwrap();
    write_tree(dir.path());
    let st = run_apply_with_failpoint(dir.path(), multi_patch(), "after_first_visible");
    assert!(!st.success());
    // One file may already be after; recover must restore all-before.
    let (code, out) = recover(dir.path());
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("rolled_back"), "{out}");
    assert_all_before(dir.path());
    assert!(status_ok(dir.path()));
}

#[test]
fn crash_matrix_before_completed_finishes_all_after() {
    let dir = tempdir().unwrap();
    write_tree(dir.path());
    let st = run_apply_with_failpoint(dir.path(), multi_patch(), "before_completed");
    assert!(!st.success());
    assert_all_after_multi(dir.path());
    let (code, out) = recover(dir.path());
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("completed"), "{out}");
    assert_all_after_multi(dir.path());
    assert!(status_ok(dir.path()));
}

#[test]
fn crash_matrix_single_file_after_first_visible_completes() {
    let dir = tempdir().unwrap();
    write_tree(dir.path());
    let st = run_apply_with_failpoint(dir.path(), single_patch(), "after_first_visible");
    assert!(!st.success());
    // Single-file: after first rename the tree is already all-after.
    let (code, out) = recover(dir.path());
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("completed"), "{out}");
    assert_eq!(fs::read_to_string(dir.path().join("a.txt")).unwrap(), "ALPHA\n");
    assert_eq!(fs::read_to_string(dir.path().join("b.txt")).unwrap(), "bravo\n");
    assert!(status_ok(dir.path()));
}
