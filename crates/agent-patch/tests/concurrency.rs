//! Concurrent modification / stale content detection.

use agent_patch::app::{run, AppConfig};
use agent_patch::error::Limits;
use std::fs;
use std::time::Duration;
use tempfile::tempdir;

#[test]
fn stale_hunk_fails_without_mutation() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("f.txt"), "alpha\n").unwrap();
    let patch_path = dir.path().join("p.patch");
    fs::write(
        &patch_path,
        "*** Begin Patch\n*** Update File: f.txt\n@@\n-beta\n+gamma\n*** End Patch\n",
    )
    .unwrap();
    let out = run(AppConfig {
        root: dir.path().to_path_buf(),
        patch_file: Some(patch_path),
        check: false,
        plan: false,
        verify: false,
        verify_argv: Vec::new(),
        verify_shell: None,
        verify_timeout: Duration::from_secs(600),
        verify_output_limit: 8 * 1024 * 1024,
        shadow_mode: agent_patch::shadow::ShadowMode::Tree,
        shadow_include_caches: false,
        match_opts: agent_patch::match_opts::MatchOptions::default(),
        idempotent: false,
        json: true,
        quiet: false,
        limits: Limits::default(),
        fsync: false,
        receipt: None,
    });
    assert_eq!(out.exit_code, 1);
    assert!(out.stdout.contains("HUNK_NOT_FOUND"));
    assert_eq!(
        fs::read_to_string(dir.path().join("f.txt")).unwrap(),
        "alpha\n"
    );
}
