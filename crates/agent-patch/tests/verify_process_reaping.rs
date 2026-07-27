//! Verify runner must kill the process group on timeout (no surviving grandchildren).

use agent_patch::error::ContentFingerprint;
use agent_patch::path_policy::{parse_repo_path, CanonicalRoot};
use agent_patch::plan::{PatchPlan, PlanSummary, PlannedChange, PlannedCreate};
use agent_patch::shadow::{materialize, ShadowOptions};
use agent_patch::verify::{run_verify, VerifyOptions};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::time::Duration;
use tempfile::tempdir;

fn tiny_plan(root: &Path) -> PatchPlan {
    let root_c = CanonicalRoot::resolve(root).unwrap();
    PatchPlan {
        root: root_c,
        entries: vec![PlannedChange::Create(PlannedCreate {
            path: parse_repo_path("b.txt").unwrap(),
            abs_path: root.join("b.txt"),
            bytes: b"b\n".to_vec(),
            after_hash: ContentFingerprint::blake3(b"b\n"),
            operation_index: 0,
            lines_added: 1,
        })],
        base_fingerprints: BTreeMap::new(),
        summary: PlanSummary::default(),
        match_evidence: vec![],
        plan_digest: "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            .into(),
        risk_warnings: vec![],
    }
}

#[cfg(unix)]
#[test]
fn verify_process_reaping() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("a.txt"), "a\n").unwrap();
    let plan = tiny_plan(root);
    let shadow = materialize(root, &plan, &ShadowOptions::default()).unwrap();

    let pid_file = shadow.shadow_root.join("grandchild.pid");
    let pid_path = pid_file.display().to_string();
    // Parent sleeps past the timeout; grandchild is backgrounded in the same process group.
    let script = format!("sleep 120 & echo $! > '{pid_path}'; sleep 120");

    let err = run_verify(
        &shadow,
        "/bin/sh",
        &["-c".into(), script],
        &plan.plan_digest,
        "reap-test",
        &VerifyOptions {
            timeout: Duration::from_millis(400),
            kill_grace: Duration::from_millis(200),
            max_stream_bytes: 64 * 1024,
        },
    )
    .unwrap_err();
    assert_eq!(err.code, agent_patch::error::ErrorCode::VerifyTimeout);

    // Wait briefly for SIGKILL to land, then assert grandchild is gone.
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let mut alive = true;
    while std::time::Instant::now() < deadline {
        let Ok(text) = fs::read_to_string(&pid_file) else {
            alive = false;
            break;
        };
        let Ok(pid) = text.trim().parse::<u32>() else {
            alive = false;
            break;
        };
        let still = std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !still {
            alive = false;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        !alive,
        "grandchild from verify timeout still alive (pid file {:?})",
        fs::read_to_string(&pid_file).ok()
    );
}
