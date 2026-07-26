//! Codex apply-patch portable scenario subset.
//!
//! Fixtures live under `tests/fixtures/codex-scenarios/`. See that README for
//! inclusion/exclusion rules relative to our unique-exact contract.

use assert_cmd::Command;
use std::fs;
use std::path::{Path, PathBuf};

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/codex-scenarios")
}

fn scenario_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<_> = fs::read_dir(fixture_root())
        .expect("codex-scenarios dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    dirs
}

fn copy_tree(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    if !src.exists() {
        return;
    }
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let ty = entry.file_type().unwrap();
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_tree(&entry.path(), &to);
        } else if ty.is_file() {
            fs::copy(entry.path(), &to).unwrap();
        }
    }
}

fn collect_files(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    if !root.exists() {
        return out;
    }
    fn walk(dir: &Path, prefix: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if entry.file_type().unwrap().is_dir() {
                walk(&path, prefix, out);
            } else if entry.file_type().unwrap().is_file() {
                let rel = path
                    .strip_prefix(prefix)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push((rel, fs::read(&path).unwrap()));
            }
        }
    }
    walk(root, root, &mut out);
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn expects_failure(name: &str) -> bool {
    name.contains("_rejects_")
        || name.contains("_requires_")
        || name.contains("_fails")
        || name.starts_with("005_")
        || name.starts_with("006_")
        || name.starts_with("007_")
        || name.starts_with("008_")
        || name.starts_with("009_")
        || name.starts_with("012_")
        || name.starts_with("013_")
}

#[test]
fn codex_scenario_subset() {
    let dirs = scenario_dirs();
    assert!(
        !dirs.is_empty(),
        "expected fixtures under tests/fixtures/codex-scenarios"
    );

    for scenario in dirs {
        let name = scenario.file_name().unwrap().to_string_lossy().to_string();
        let patch = fs::read_to_string(scenario.join("patch.txt"))
            .unwrap_or_else(|e| panic!("{name}: read patch.txt: {e}"));
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        copy_tree(&scenario.join("input"), root);

        let mut cmd = Command::cargo_bin("agent-patch").unwrap();
        let assert = cmd.arg("--root").arg(root).write_stdin(patch).assert();

        if expects_failure(&name) {
            assert.failure();
            let after = collect_files(root);
            let expected = collect_files(&scenario.join("expected"));
            assert_eq!(
                after, expected,
                "failure scenario {name} must leave tree matching expected/"
            );
        } else {
            assert.success();
            let after = collect_files(root);
            let expected = collect_files(&scenario.join("expected"));
            assert_eq!(after, expected, "success scenario {name} tree mismatch");
        }
    }
}
