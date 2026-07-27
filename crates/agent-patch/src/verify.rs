//! Bounded verify command runner (process-group aware on Unix).

use crate::error::{ErrorCode, PublicError};
use crate::shadow::ShadowWorkspace;
use serde::Serialize;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct VerifyOptions {
    pub timeout: Duration,
    pub kill_grace: Duration,
    pub max_stream_bytes: u64,
}

impl Default for VerifyOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(600),
            kill_grace: Duration::from_secs(5),
            max_stream_bytes: 8 * 1024 * 1024,
        }
    }
}

/// Parse `--verify-timeout`: bare seconds, or `Ns` / `Nm` / `Nh`.
pub fn parse_verify_timeout(raw: &str) -> Result<Duration, PublicError> {
    let s = raw.trim();
    if s.is_empty() {
        return Err(PublicError::new(
            ErrorCode::InputError,
            "--verify-timeout must be a positive duration (e.g. 600, 60s, 10m).",
        ));
    }
    let (num, mult) = if let Some(rest) = s.strip_suffix('s').or_else(|| s.strip_suffix('S')) {
        (rest, 1u64)
    } else if let Some(rest) = s.strip_suffix('m').or_else(|| s.strip_suffix('M')) {
        (rest, 60)
    } else if let Some(rest) = s.strip_suffix('h').or_else(|| s.strip_suffix('H')) {
        (rest, 3600)
    } else {
        (s, 1)
    };
    let n: u64 = num.trim().parse().map_err(|_| {
        PublicError::new(
            ErrorCode::InputError,
            format!("Invalid --verify-timeout {raw:?}; use seconds or Ns/Nm/Nh."),
        )
    })?;
    if n == 0 {
        return Err(PublicError::new(
            ErrorCode::InputError,
            "--verify-timeout must be greater than zero.",
        ));
    }
    n.checked_mul(mult)
        .map(Duration::from_secs)
        .ok_or_else(|| PublicError::new(ErrorCode::InputError, "--verify-timeout overflowed."))
}

/// Parse `--verify-output-limit` as a positive byte count.
pub fn parse_verify_output_limit(raw: &str) -> Result<u64, PublicError> {
    let n: u64 = raw.trim().parse().map_err(|_| {
        PublicError::new(
            ErrorCode::InputError,
            format!("Invalid --verify-output-limit {raw:?}; expected positive byte count."),
        )
    })?;
    if n == 0 {
        return Err(PublicError::new(
            ErrorCode::InputError,
            "--verify-output-limit must be greater than zero.",
        ));
    }
    Ok(n)
}

#[derive(Debug, Serialize)]
pub struct VerifyReport {
    pub ok: bool,
    pub exit_code: Option<i32>,
    pub signalled: bool,
    pub timed_out: bool,
    pub duration_ms: u64,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub stdout_path: String,
    pub stderr_path: String,
    pub representative: bool,
    pub shadow_mode: String,
}

pub fn run_verify(
    shadow: &ShadowWorkspace,
    program: &str,
    args: &[String],
    plan_digest: &str,
    invocation_id: &str,
    opts: &VerifyOptions,
) -> Result<VerifyReport, PublicError> {
    let art_dir = shadow.shadow_root.join(".agent-patch-verify");
    fs::create_dir_all(&art_dir).map_err(|e| {
        PublicError::new(
            ErrorCode::IoError,
            format!("Cannot create verify artifact dir: {e}"),
        )
    })?;
    let stdout_path = art_dir.join("stdout.log");
    let stderr_path = art_dir.join("stderr.log");

    let mut cmd = Command::new(program);
    cmd.args(args)
        .current_dir(&shadow.shadow_root)
        .env("AGENT_PATCH_MODE", "verify")
        .env(
            "AGENT_PATCH_REAL_ROOT",
            shadow.real_root.display().to_string(),
        )
        .env(
            "AGENT_PATCH_SHADOW_ROOT",
            shadow.shadow_root.display().to_string(),
        )
        .env("AGENT_PATCH_PLAN_DIGEST", plan_digest)
        .env("AGENT_PATCH_INVOCATION_ID", invocation_id)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            cmd.pre_exec(|| {
                // New process group so we can kill the whole tree.
                libc::setpgid(0, 0);
                Ok(())
            });
        }
    }

    let started = Instant::now();
    let mut child = cmd.spawn().map_err(|e| {
        PublicError::new(
            ErrorCode::VerifyFailed,
            format!("Cannot spawn verify command {program}: {e}"),
        )
    })?;

    let max_bytes = opts.max_stream_bytes;
    let stdout_path_t = stdout_path.clone();
    let stderr_path_t = stderr_path.clone();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let t_out = std::thread::spawn(move || drain_bounded(stdout, &stdout_path_t, max_bytes));
    let t_err = std::thread::spawn(move || drain_bounded(stderr, &stderr_path_t, max_bytes));

    let mut timed_out = false;
    let mut signalled = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st,
            Ok(None) => {
                if started.elapsed() >= opts.timeout {
                    timed_out = true;
                    kill_tree(&mut child, opts.kill_grace);
                    match child.wait() {
                        Ok(st) => break st,
                        Err(e) => {
                            return Err(PublicError::new(
                                ErrorCode::VerifyTimeout,
                                format!("Verify wait after kill failed: {e}"),
                            ));
                        }
                    }
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => {
                return Err(PublicError::new(
                    ErrorCode::VerifyFailed,
                    format!("Verify wait failed: {e}"),
                ));
            }
        }
    };

    let (_, stdout_trunc) = t_out.join().map_err(|_| {
        PublicError::new(ErrorCode::InternalError, "stdout drain thread panicked")
    })??;
    let (_, stderr_trunc) = t_err.join().map_err(|_| {
        PublicError::new(ErrorCode::InternalError, "stderr drain thread panicked")
    })??;

    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if status.signal().is_some() {
            signalled = true;
        }
    }

    let exit_code = status.code();
    let ok = !timed_out && !signalled && exit_code == Some(0);
    let code = if timed_out {
        ErrorCode::VerifyTimeout
    } else if signalled {
        ErrorCode::VerifySignalled
    } else if !ok {
        ErrorCode::VerifyFailed
    } else {
        // success path returns report with ok=true
        ErrorCode::InternalError
    };

    let report = VerifyReport {
        ok,
        exit_code,
        signalled,
        timed_out,
        duration_ms: started.elapsed().as_millis() as u64,
        stdout_truncated: stdout_trunc,
        stderr_truncated: stderr_trunc,
        stdout_path: stdout_path.display().to_string(),
        stderr_path: stderr_path.display().to_string(),
        representative: shadow.report.representative,
        shadow_mode: match shadow.report.shadow_mode {
            crate::shadow::ShadowMode::Tree => "tree".into(),
            crate::shadow::ShadowMode::Touched => "touched".into(),
        },
    };

    if ok {
        Ok(report)
    } else {
        let msg = if timed_out {
            format!("Verify timed out after {}s", opts.timeout.as_secs())
        } else if signalled {
            "Verify process signalled".to_string()
        } else {
            format!(
                "Verify command exited {}",
                exit_code
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "?".into())
            )
        };
        Err(PublicError::new(code, msg).with_hint(
            "Shadow discarded; real root unchanged. Inspect verify artifacts if retained.",
        ))
    }
}

fn drain_bounded(
    stream: Option<impl Read>,
    path: &Path,
    max_bytes: u64,
) -> Result<(u64, bool), PublicError> {
    let mut file = File::create(path).map_err(|e| {
        PublicError::new(
            ErrorCode::IoError,
            format!("Cannot create {}: {e}", path.display()),
        )
    })?;
    let Some(mut stream) = stream else {
        return Ok((0, false));
    };
    let mut buf = [0u8; 8192];
    let mut total = 0u64;
    let mut truncated = false;
    loop {
        let n = match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => {
                return Err(PublicError::new(
                    ErrorCode::IoError,
                    format!("Cannot read verify stream: {e}"),
                ));
            }
        };
        let n = n as u64;
        if total >= max_bytes {
            truncated = true;
            // Keep draining to avoid blocking the child, but stop writing.
            continue;
        }
        let remain = max_bytes - total;
        let write_n = (n as usize).min(remain as usize);
        file.write_all(&buf[..write_n]).map_err(|e| {
            PublicError::new(ErrorCode::IoError, format!("Cannot write artifact: {e}"))
        })?;
        total += write_n as u64;
        if (write_n as u64) < n {
            truncated = true;
        }
    }
    Ok((total, truncated))
}

fn kill_tree(child: &mut std::process::Child, grace: Duration) {
    #[cfg(unix)]
    {
        let pid = child.id() as i32;
        unsafe {
            libc::kill(-pid, libc::SIGTERM);
        }
        let deadline = Instant::now() + grace;
        while Instant::now() < deadline {
            if let Ok(Some(_)) = child.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        unsafe {
            libc::kill(-pid, libc::SIGKILL);
        }
    }
    #[cfg(not(unix))]
    {
        let _ = grace;
        let _ = child.kill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ContentFingerprint;
    use crate::path_policy::{parse_repo_path, CanonicalRoot};
    use crate::plan::{PatchPlan, PlanSummary, PlannedChange, PlannedCreate};
    use crate::shadow::{materialize, ShadowOptions};
    use std::collections::BTreeMap;

    #[test]
    fn verify_true_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("a.txt"), "a\n").unwrap();
        let root_c = CanonicalRoot::resolve(root).unwrap();
        let plan = PatchPlan {
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
        };
        let shadow = materialize(root, &plan, &ShadowOptions::default()).unwrap();
        let report = run_verify(
            &shadow,
            "true",
            &[],
            &plan.plan_digest,
            "inv1",
            &VerifyOptions {
                timeout: Duration::from_secs(5),
                ..VerifyOptions::default()
            },
        )
        .unwrap();
        assert!(report.ok);
        assert!(report.representative);
    }

    #[test]
    fn verify_false_fails_without_root_mutation() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("a.txt"), "a\n").unwrap();
        let root_c = CanonicalRoot::resolve(root).unwrap();
        let plan = PatchPlan {
            root: root_c,
            entries: vec![],
            base_fingerprints: BTreeMap::new(),
            summary: PlanSummary::default(),
            match_evidence: vec![],
            plan_digest: "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .into(),
            risk_warnings: vec![],
        };
        let shadow = materialize(root, &plan, &ShadowOptions::default()).unwrap();
        let err = run_verify(
            &shadow,
            "false",
            &[],
            &plan.plan_digest,
            "inv2",
            &VerifyOptions {
                timeout: Duration::from_secs(5),
                ..VerifyOptions::default()
            },
        )
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::VerifyFailed);
        assert_eq!(fs::read_to_string(root.join("a.txt")).unwrap(), "a\n");
    }
}
