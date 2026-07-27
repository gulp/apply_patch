//! Machine-mode argv coaching / rewrite integration tests.

use serde_json::Value;
use std::fs;
use std::process::Command;
use tempfile::tempdir;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_agent-patch"))
}

#[test]
fn r2_toplevel_json_root_before_status_emits_coach() {
    let dir = tempdir().unwrap();
    let out = bin()
        .args([
            "--json",
            "--root",
            dir.path().to_str().unwrap(),
            "status",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr={}", String::from_utf8_lossy(&out.stderr));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true);
    assert!(v.get("coach").is_some(), "stdout={}", v);
    assert!(
        v["coach"]["note"]
            .as_str()
            .unwrap_or("")
            .contains("subcommand"),
        "coach={}",
        v["coach"]
    );
}

#[test]
fn e1_verify_without_argv_emits_examples() {
    let dir = tempdir().unwrap();
    let patch = dir.path().join("c.patch");
    fs::write(
        &patch,
        "*** Begin Patch\n*** Add File: a.txt\n+hi\n*** End Patch\n",
    )
    .unwrap();
    let out = bin()
        .args([
            "--json",
            "--root",
            dir.path().to_str().unwrap(),
            "--verify",
            patch.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"]["code"], "INPUT_ERROR");
    let examples = v["error"]["examples"].as_array().unwrap();
    assert!(examples.len() >= 2, "examples={examples:?}");
}

#[test]
fn invented_revert_flag_suggests_subcommand() {
    let out = bin()
        .args(["--json", "--revert", "receipt.json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"]["code"], "INPUT_ERROR");
    let examples = v["error"]["examples"].as_array().unwrap();
    assert!(examples.len() >= 2);
    let sug = v["error"]["suggestions"].as_array().cloned().unwrap_or_default();
    assert!(
        sug.iter().any(|s| s.as_str() == Some("revert")),
        "suggestions={sug:?}"
    );
}

#[test]
fn robot_docs_json() {
    let out = bin().args(["--robot", "robot-docs"]).output().unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true);
    assert!(v["guide"].as_str().unwrap_or("").contains("Footguns"));
}

#[test]
fn r1_verify_patch_after_dashdash_rewrites() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    let patch = dir.path().join("c.patch");
    fs::write(
        &patch,
        "*** Begin Patch\n*** Update File: a.txt\n@@\n-one\n+two\n*** End Patch\n",
    )
    .unwrap();
    let out = bin()
        .args([
            "--json",
            "--root",
            dir.path().to_str().unwrap(),
            "--verify",
            "--",
            "true",
            patch.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true);
    assert!(
        v["coach"]["note"]
            .as_str()
            .unwrap_or("")
            .contains("patch path"),
        "coach={}",
        v["coach"]
    );
    assert_eq!(fs::read_to_string(dir.path().join("a.txt")).unwrap(), "two\n");
}
