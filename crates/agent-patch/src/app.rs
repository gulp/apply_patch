//! Application service — parse → validate → snapshot → plan → (check|commit).

use crate::commit::commit_plan;
use crate::diagnostics::{
    emit_error_human, emit_error_json, emit_success_human, JsonFileResult, JsonSuccess, JsonSummary,
};
use crate::error::{Limits, PublicError};
use crate::fs::{FileSystem, RealFileSystem};
use crate::input::read_patch_bytes;
use crate::path_policy::{check_path_collisions, CanonicalRoot};
use crate::plan::{build_plan, PlannedChange};
use crate::protocol::parse_patch;
use crate::snapshot::load_snapshots;
use crate::telemetry::{debug_log, InvocationTimers};
use crate::validate::{validate_against_snapshots, validate_document};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub root: PathBuf,
    pub patch_file: Option<PathBuf>,
    pub check: bool,
    pub json: bool,
    pub quiet: bool,
    pub limits: Limits,
    pub fsync: bool,
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
            let stdout = if config.json {
                serde_json::to_string_pretty(&success).unwrap_or_else(|_| {
                    r#"{"version":1,"ok":false,"error":{"code":"INTERNAL_ERROR","exit_code":6,"message":"JSON serialization failed"}}"#.to_string()
                })
            } else if config.quiet {
                String::new()
            } else {
                emit_success_human(&success.summary, config.check)
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
    validate_against_snapshots(&ops, &snapshots)?;

    let plan = build_plan(root.clone(), &ops, &snapshots)?;
    debug_log("plan", &format!("entries={}", plan.entries.len()));

    if !config.check {
        commit_plan(&fs as &dyn FileSystem, &plan, &config.limits)?;
        debug_log("commit", "ok");
    }

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

    Ok(JsonSuccess {
        version: 1,
        ok: true,
        mode: if config.check {
            "check".into()
        } else {
            "apply".into()
        },
        root: root.path.display().to_string(),
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
    })
}

pub fn default_root() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

#[allow(dead_code)]
pub fn root_from(path: &Path) -> PathBuf {
    path.to_path_buf()
}
