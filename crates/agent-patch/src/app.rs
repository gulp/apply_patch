//! Application service — parse → validate → snapshot → plan → (check|plan|commit).

use crate::commit::commit_plan;
use crate::diagnostics::{
    emit_error_human, emit_error_json, emit_success_human, JsonFileResult, JsonSuccess, JsonSummary,
};
use crate::error::{ErrorCode, Limits, PublicError};
use crate::events::{self, EventRecord};
use crate::fs::{FileSystem, RealFileSystem};
use crate::input::read_patch_bytes;
use crate::match_opts::MatchOptions;
use crate::path_policy::{check_path_collisions, CanonicalRoot};
use crate::plan::{build_plan_with, execution_plan_json, PlannedChange};
use crate::protocol::parse_patch;
use crate::shadow::{materialize, ShadowMode, ShadowOptions};
use crate::snapshot::load_snapshots;
use crate::telemetry::{debug_log, InvocationTimers};
use crate::validate::{validate_against_snapshots, validate_document};
use crate::verify::{run_verify, VerifyOptions};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub root: PathBuf,
    pub patch_file: Option<PathBuf>,
    pub check: bool,
    pub plan: bool,
    pub verify: bool,
    pub verify_argv: Vec<String>,
    pub verify_shell: Option<String>,
    pub verify_timeout: Duration,
    pub verify_output_limit: u64,
    pub shadow_mode: ShadowMode,
    pub shadow_include_caches: bool,
    pub match_opts: MatchOptions,
    pub idempotent: bool,
    pub json: bool,
    pub quiet: bool,
    pub limits: Limits,
    pub fsync: bool,
    pub receipt: Option<PathBuf>,
}

#[derive(Debug)]
pub struct AppOutput {
    pub exit_code: u8,
    pub stdout: String,
    pub stderr: String,
}

pub fn run(config: AppConfig) -> AppOutput {
    let timers = InvocationTimers::start();
    match run_inner(&config, &timers) {
        Ok(success) => {
            let mode = success.mode.as_str();
            let stdout = if config.json {
                serde_json::to_string_pretty(&success).unwrap_or_else(|_| {
                    r#"{"version":1,"ok":false,"error":{"code":"INTERNAL_ERROR","exit_code":6,"message":"JSON serialization failed"}}"#.to_string()
                })
            } else if config.quiet {
                String::new()
            } else {
                emit_success_human(&success.summary, mode, &success.warnings)
            };
            AppOutput {
                exit_code: 0,
                stdout,
                stderr: String::new(),
            }
        }
        Err(err) => {
            if config.json {
                AppOutput {
                    exit_code: err.exit_code(),
                    stdout: emit_error_json(&err),
                    stderr: String::new(),
                }
            } else {
                AppOutput {
                    exit_code: err.exit_code(),
                    stdout: String::new(),
                    stderr: emit_error_human(&err),
                }
            }
        }
    }
}

fn run_inner(config: &AppConfig, timers: &InvocationTimers) -> Result<JsonSuccess, PublicError> {
    let bytes = read_patch_bytes(config.patch_file.as_deref(), config.limits.max_patch_bytes)?;
    debug_log("input", &format!("bytes={}", bytes.len()));

    let text = std::str::from_utf8(&bytes).map_err(|_| {
        PublicError::new(
            crate::error::ErrorCode::InvalidUtf8,
            "Patch input is not valid UTF-8.",
        )
    })?;

    let doc = parse_patch(text)?;
    debug_log("parse", &format!("ops={}", doc.operations.len()));

    let ops = validate_document(&doc, &config.limits)?;
    let paths: Vec<_> = ops.iter().map(|(_, p, _)| p.clone()).collect();

    let root = CanonicalRoot::resolve(&config.root)?;
    check_path_collisions(&root, &paths)?;

    let fs = RealFileSystem {
        fsync: config.fsync,
    };
    let snapshots = load_snapshots(&fs, &root, &paths, &config.limits)?;

    if config.idempotent {
        match assess_idempotent(&ops, &snapshots)? {
            IdempotentStatus::FullyApplied => {
                return Ok(JsonSuccess {
                    version: 2,
                    ok: true,
                    mode: "apply".into(),
                    root: root.path.display().to_string(),
                    plan_digest: None,
                    transaction_id: None,
                    summary: JsonSummary {
                        files_total: ops.len(),
                        files_added: 0,
                        files_updated: 0,
                        files_deleted: 0,
                        hunks_applied: 0,
                        lines_added: 0,
                        lines_deleted: 0,
                        duration_ms: timers.elapsed_ms(),
                    },
                    files: Vec::new(),
                    plan: None,
                    verify: None,
                    already_applied: true,
                    warnings: Vec::new(),
                });
            }
            IdempotentStatus::Partial => {
                return Err(PublicError::new(
                    ErrorCode::PartiallyApplied,
                    "Patch is only partially applied; refusing mixed replay under --idempotent.",
                ));
            }
            IdempotentStatus::NotApplied => {}
        }
    }

    validate_against_snapshots(&ops, &snapshots)?;

    let plan = build_plan_with(root.clone(), &ops, &snapshots, config.match_opts)?;
    debug_log("plan", &format!("entries={}", plan.entries.len()));

    if config.verify {
        if config.verify_shell.is_none() && config.verify_argv.is_empty() {
            return Err(PublicError::new(
                ErrorCode::InputError,
                "--verify requires a command after `--` (e.g. --verify -- true).",
            ));
        }
        let shadow_opts = ShadowOptions {
            mode: config.shadow_mode,
            include_caches: config.shadow_include_caches,
            ..ShadowOptions::default()
        };
        let shadow = materialize(&root.path, &plan, &shadow_opts)?;
        debug_log("shadow", &format!("files={}", shadow.report.files_copied));
        let (program, args_owned): (String, Vec<String>) =
            if let Some(script) = &config.verify_shell {
                ("/bin/sh".into(), vec!["-c".into(), script.clone()])
            } else {
                (
                    config.verify_argv[0].clone(),
                    config.verify_argv[1..].to_vec(),
                )
            };
        let verify_report = match run_verify(
            &shadow,
            &program,
            &args_owned,
            &plan.plan_digest,
            &format!("{}", std::process::id()),
            &VerifyOptions {
                timeout: config.verify_timeout,
                max_stream_bytes: config.verify_output_limit,
                ..VerifyOptions::default()
            },
        ) {
            Ok(r) => r,
            Err(e) => {
                events::emit(
                    &root.path,
                    &EventRecord {
                        version: 2,
                        ts: events::now_ts(),
                        phase: "verify",
                        ok: false,
                        transaction_id: None,
                        plan_digest: Some(&plan.plan_digest),
                        detail: Some(e.code.as_str()),
                    },
                );
                return Err(e);
            }
        };
        // Promote: journaled commit to real root (lock acquired inside).
        let result = commit_plan(
            &fs as &dyn FileSystem,
            &plan,
            &config.limits,
            config.receipt.as_deref(),
        )?;
        debug_log("commit", "ok");
        events::emit(
            &root.path,
            &EventRecord {
                version: 2,
                ts: events::now_ts(),
                phase: "verify",
                ok: true,
                transaction_id: Some(&result.transaction_id),
                plan_digest: Some(&plan.plan_digest),
                detail: None,
            },
        );
        return Ok(build_success(
            config,
            &root,
            &plan,
            "verify",
            Some(result.transaction_id),
            timers,
            Some(verify_report),
            false,
        ));
    }

    let read_only = config.check || config.plan;
    let transaction_id = if !read_only {
        let result = commit_plan(
            &fs as &dyn FileSystem,
            &plan,
            &config.limits,
            config.receipt.as_deref(),
        )?;
        debug_log("commit", "ok");
        events::emit(
            &root.path,
            &EventRecord {
                version: 2,
                ts: events::now_ts(),
                phase: "apply",
                ok: true,
                transaction_id: Some(&result.transaction_id),
                plan_digest: Some(&plan.plan_digest),
                detail: None,
            },
        );
        Some(result.transaction_id)
    } else {
        None
    };

    let mode = if config.plan {
        "plan"
    } else if config.check {
        "check"
    } else {
        "apply"
    };
    Ok(build_success(
        config,
        &root,
        &plan,
        mode,
        transaction_id,
        timers,
        None,
        false,
    ))
}

enum IdempotentStatus {
    FullyApplied,
    Partial,
    NotApplied,
}

fn assess_idempotent(
    ops: &[(
        usize,
        crate::path_policy::RepoPath,
        &crate::protocol::ast::FileOperation,
    )],
    snapshots: &std::collections::BTreeMap<
        crate::path_policy::RepoPath,
        crate::snapshot::FileSnapshot,
    >,
) -> Result<IdempotentStatus, PublicError> {
    use crate::engine::matcher::find_all_matches;
    use crate::engine::{apply_update_with, split_content_lines};
    use crate::protocol::ast::FileOperation;
    use crate::snapshot::FileState;

    let mut any_applied = false;
    let mut any_pending = false;
    for (_, repo_path, op) in ops {
        let snap = snapshots.get(repo_path).ok_or_else(|| {
            PublicError::new(ErrorCode::InternalError, "Missing snapshot for path.")
        })?;
        match (*op, &snap.state) {
            (FileOperation::Add(add), FileState::Missing) => {
                let _ = add;
                any_pending = true;
            }
            (FileOperation::Add(add), FileState::Present(p)) => {
                if p.text == add.content {
                    any_applied = true;
                } else {
                    return Ok(IdempotentStatus::Partial);
                }
            }
            (FileOperation::Delete(_), FileState::Missing) => any_applied = true,
            (FileOperation::Delete(_), FileState::Present(_)) => any_pending = true,
            (FileOperation::Update(_update), FileState::Missing) => {
                return Ok(IdempotentStatus::Partial);
            }
            (FileOperation::Update(update), FileState::Present(p)) => {
                match apply_update_with(
                    &p.text,
                    update,
                    "\n",
                    p.final_newline,
                    MatchOptions::default(),
                ) {
                    Ok(_) => any_pending = true,
                    Err(e) if e.code == ErrorCode::PatchNoEffect => any_applied = true,
                    Err(e)
                        if e.code == ErrorCode::HunkNotFound
                            || e.code == ErrorCode::HunkAmbiguous =>
                    {
                        let lines = split_content_lines(&p.text);
                        let refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
                        let mut cursor = 0usize;
                        let mut all_new = true;
                        for hunk in &update.hunks {
                            let new = hunk.new_text_lines();
                            if new.is_empty() {
                                continue;
                            }
                            let hits = find_all_matches(&refs, &new, cursor);
                            if hits.len() != 1 {
                                all_new = false;
                                break;
                            }
                            cursor = hits[0].1;
                        }
                        if all_new {
                            any_applied = true;
                        } else {
                            return Ok(IdempotentStatus::Partial);
                        }
                    }
                    Err(e) => return Err(e),
                }
            }
        }
    }
    Ok(if any_applied && any_pending {
        IdempotentStatus::Partial
    } else if any_applied {
        IdempotentStatus::FullyApplied
    } else {
        IdempotentStatus::NotApplied
    })
}

#[allow(clippy::too_many_arguments)]
fn build_success(
    config: &AppConfig,
    root: &CanonicalRoot,
    plan: &crate::plan::PatchPlan,
    mode: &str,
    transaction_id: Option<String>,
    timers: &InvocationTimers,
    verify_report: Option<crate::verify::VerifyReport>,
    already_applied: bool,
) -> JsonSuccess {
    let files = plan
        .entries
        .iter()
        .map(|e| match e {
            PlannedChange::Create(c) => JsonFileResult {
                path: c.path.as_str().to_string(),
                operation: "add".into(),
                hunks: 0,
                lines_added: c.lines_added,
                lines_deleted: 0,
                before_blake3: None,
                after_blake3: Some(c.after_hash.hex()),
            },
            PlannedChange::Modify(m) => JsonFileResult {
                path: m.path.as_str().to_string(),
                operation: "update".into(),
                hunks: m.hunks,
                lines_added: m.counts.lines_added,
                lines_deleted: m.counts.lines_deleted,
                before_blake3: Some(m.before.fingerprint.hex()),
                after_blake3: Some(m.after_hash.hex()),
            },
            PlannedChange::Remove(r) => JsonFileResult {
                path: r.path.as_str().to_string(),
                operation: "delete".into(),
                hunks: 0,
                lines_added: 0,
                lines_deleted: r.lines_deleted,
                before_blake3: Some(r.before.fingerprint.hex()),
                after_blake3: None,
            },
        })
        .collect();

    let (version, plan_json, plan_digest) = if config.plan {
        (
            2u32,
            Some(execution_plan_json(plan, true)),
            Some(plan.plan_digest.clone()),
        )
    } else if verify_report.is_some() || !plan.risk_warnings.is_empty() {
        (2u32, None, Some(plan.plan_digest.clone()))
    } else {
        (1u32, None, Some(plan.plan_digest.clone()))
    };

    JsonSuccess {
        version,
        ok: true,
        mode: mode.into(),
        root: root.path.display().to_string(),
        plan_digest,
        transaction_id,
        summary: JsonSummary {
            files_total: plan.summary.files_total,
            files_added: plan.summary.files_added,
            files_updated: plan.summary.files_updated,
            files_deleted: plan.summary.files_deleted,
            hunks_applied: plan.summary.hunks_applied,
            lines_added: plan.summary.lines_added,
            lines_deleted: plan.summary.lines_deleted,
            duration_ms: timers.elapsed_ms(),
        },
        files,
        plan: plan_json,
        verify: verify_report,
        already_applied,
        warnings: plan.risk_warnings.clone(),
    }
}

pub fn default_root() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

#[allow(dead_code)]
pub fn root_from(path: &Path) -> PathBuf {
    path.to_path_buf()
}
