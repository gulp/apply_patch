//! Patch planner — pairs operations with applied in-memory results.

use crate::engine::{apply_update_with, locate_chunks_with, unified_diff, DiffCounts};
use crate::error::{ContentFingerprint, ErrorCode, NewlineStyle, PublicError};
use crate::match_opts::{enforce_risk, MatchOptions};
use crate::oracle::MatchEvidence;
use crate::path_policy::{CanonicalRoot, RepoPath};
use crate::protocol::ast::FileOperation;
use crate::snapshot::{FileSnapshot, FileState, PresentFile};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct PatchPlan {
    pub root: CanonicalRoot,
    pub entries: Vec<PlannedChange>,
    pub base_fingerprints: BTreeMap<RepoPath, Option<ContentFingerprint>>,
    pub summary: PlanSummary,
    pub match_evidence: Vec<MatchEvidence>,
    pub plan_digest: String,
    /// Populated when `--risk=warn` and findings exist.
    pub risk_warnings: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct PlanSummary {
    pub files_total: usize,
    pub files_added: usize,
    pub files_updated: usize,
    pub files_deleted: usize,
    pub hunks_applied: usize,
    pub lines_added: usize,
    pub lines_deleted: usize,
}

#[derive(Debug, Clone)]
pub enum PlannedChange {
    Create(PlannedCreate),
    Modify(PlannedModify),
    Remove(PlannedRemove),
}

impl PlannedChange {
    pub fn path(&self) -> &RepoPath {
        match self {
            Self::Create(c) => &c.path,
            Self::Modify(m) => &m.path,
            Self::Remove(r) => &r.path,
        }
    }

    pub fn abs_path(&self) -> &std::path::Path {
        match self {
            Self::Create(c) => &c.abs_path,
            Self::Modify(m) => &m.abs_path,
            Self::Remove(r) => &r.abs_path,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlannedCreate {
    pub path: RepoPath,
    pub abs_path: std::path::PathBuf,
    pub bytes: Vec<u8>,
    pub after_hash: ContentFingerprint,
    pub operation_index: usize,
    pub lines_added: usize,
}

#[derive(Debug, Clone)]
pub struct PlannedModify {
    pub path: RepoPath,
    pub abs_path: std::path::PathBuf,
    pub before: PresentFile,
    pub after_bytes: Vec<u8>,
    pub after_hash: ContentFingerprint,
    pub operation_index: usize,
    pub hunks: usize,
    pub counts: DiffCounts,
    pub evidence: Vec<MatchEvidence>,
}

#[derive(Debug, Clone)]
pub struct PlannedRemove {
    pub path: RepoPath,
    pub abs_path: std::path::PathBuf,
    pub before: PresentFile,
    pub operation_index: usize,
    pub lines_deleted: usize,
}

#[derive(Debug, Serialize)]
pub struct ExecutionPlanJson {
    pub version: u32,
    pub root: String,
    pub plan_digest: String,
    pub entries: Vec<ExecutionPlanEntryJson>,
    pub match_evidence: Vec<MatchEvidence>,
}

#[derive(Debug, Serialize)]
pub struct ExecutionPlanEntryJson {
    pub path: String,
    pub operation: String,
    pub before_blake3: Option<String>,
    pub after_blake3: Option<String>,
    pub hunks: usize,
    pub lines_added: usize,
    pub lines_deleted: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unified_diff: Option<String>,
}

pub fn build_plan(
    root: CanonicalRoot,
    ops: &[(usize, RepoPath, &FileOperation)],
    snapshots: &BTreeMap<RepoPath, FileSnapshot>,
) -> Result<PatchPlan, PublicError> {
    build_plan_with(root, ops, snapshots, MatchOptions::default())
}

pub fn build_plan_with(
    root: CanonicalRoot,
    ops: &[(usize, RepoPath, &FileOperation)],
    snapshots: &BTreeMap<RepoPath, FileSnapshot>,
    opts: MatchOptions,
) -> Result<PatchPlan, PublicError> {
    let mut entries = Vec::new();
    let mut base_fingerprints = BTreeMap::new();
    let mut match_evidence = Vec::new();
    let mut summary = PlanSummary {
        files_total: ops.len(),
        ..Default::default()
    };

    for (index, repo_path, op) in ops {
        let snap = snapshots.get(repo_path).unwrap();
        match op {
            FileOperation::Add(add) => {
                base_fingerprints.insert(repo_path.clone(), None);
                let bytes = add.content.as_bytes().to_vec();
                let after_hash = ContentFingerprint::blake3(&bytes);
                let lines_added = add.content.lines().count();
                summary.files_added += 1;
                summary.lines_added += lines_added;
                entries.push(PlannedChange::Create(PlannedCreate {
                    path: repo_path.clone(),
                    abs_path: snap.abs_path.clone(),
                    bytes,
                    after_hash,
                    operation_index: *index,
                    lines_added,
                }));
            }
            FileOperation::Update(update) => {
                let present = match &snap.state {
                    FileState::Present(p) => p,
                    FileState::Missing => unreachable!("validated"),
                };
                if let Some(pin) = update.hash_pin.as_deref() {
                    if present.fingerprint.hex() != pin {
                        return Err(PublicError::new(
                            ErrorCode::HashPinMismatch,
                            format!(
                                "Hash pin mismatch for {}: expected blake3 {pin}, got {}",
                                repo_path,
                                present.fingerprint.hex()
                            ),
                        )
                        .with_path(repo_path.as_str())
                        .with_operation(*index)
                        .with_source(update.source_span));
                    }
                }
                base_fingerprints.insert(repo_path.clone(), Some(present.fingerprint.clone()));

                let newline = match present.newline_style {
                    NewlineStyle::CrLf => "\r\n",
                    NewlineStyle::Lf | NewlineStyle::None => "\n",
                    NewlineStyle::Mixed => {
                        return Err(PublicError::new(
                            ErrorCode::MixedLineEndings,
                            "Mixed line endings.",
                        )
                        .with_path(repo_path.as_str()));
                    }
                };

                let applied =
                    apply_update_with(&present.text, update, newline, present.final_newline, opts)
                        .map_err(|e| e.with_operation(*index))?;

                let lines = crate::engine::split_content_lines(&present.text);
                let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
                let chunks = locate_chunks_with(&line_refs, update, opts)
                    .map_err(|e| e.with_operation(*index))?;
                let ev: Vec<_> = chunks.into_iter().map(|c| c.evidence).collect();
                match_evidence.extend(ev.iter().cloned());

                let mut after_text = applied.text;
                if present.text.starts_with('\u{FEFF}') && !after_text.starts_with('\u{FEFF}') {
                    after_text.insert(0, '\u{FEFF}');
                }

                let after_bytes = after_text.into_bytes();
                let after_hash = ContentFingerprint::blake3(&after_bytes);
                summary.files_updated += 1;
                summary.hunks_applied += applied.hunks_applied;
                summary.lines_added += applied.counts.lines_added;
                summary.lines_deleted += applied.counts.lines_deleted;
                entries.push(PlannedChange::Modify(PlannedModify {
                    path: repo_path.clone(),
                    abs_path: snap.abs_path.clone(),
                    before: present.clone(),
                    after_bytes,
                    after_hash,
                    operation_index: *index,
                    hunks: applied.hunks_applied,
                    counts: applied.counts,
                    evidence: ev,
                }));
            }
            FileOperation::Delete(_) => {
                let present = match &snap.state {
                    FileState::Present(p) => p,
                    FileState::Missing => unreachable!("validated"),
                };
                base_fingerprints.insert(repo_path.clone(), Some(present.fingerprint.clone()));
                let lines_deleted = present.text.lines().count();
                summary.files_deleted += 1;
                summary.lines_deleted += lines_deleted;
                entries.push(PlannedChange::Remove(PlannedRemove {
                    path: repo_path.clone(),
                    abs_path: snap.abs_path.clone(),
                    before: present.clone(),
                    operation_index: *index,
                    lines_deleted,
                }));
            }
        }
    }

    let risk_warnings = enforce_risk(&match_evidence, opts.risk)?;

    entries.sort_by(|a, b| a.path().cmp(b.path()));
    let plan_digest = compute_plan_digest(&root, &entries);

    Ok(PatchPlan {
        root,
        entries,
        base_fingerprints,
        summary,
        match_evidence,
        plan_digest,
        risk_warnings,
    })
}

fn compute_plan_digest(root: &CanonicalRoot, entries: &[PlannedChange]) -> String {
    let mut owned: Vec<serde_json::Value> = Vec::new();
    for e in entries {
        match e {
            PlannedChange::Create(c) => {
                owned.push(serde_json::json!({
                    "path": c.path.as_str(),
                    "operation": "add",
                    "before_blake3": null,
                    "after_blake3": c.after_hash.hex(),
                }));
            }
            PlannedChange::Modify(m) => {
                owned.push(serde_json::json!({
                    "path": m.path.as_str(),
                    "operation": "update",
                    "before_blake3": m.before.fingerprint.hex(),
                    "after_blake3": m.after_hash.hex(),
                }));
            }
            PlannedChange::Remove(r) => {
                owned.push(serde_json::json!({
                    "path": r.path.as_str(),
                    "operation": "delete",
                    "before_blake3": r.before.fingerprint.hex(),
                    "after_blake3": null,
                }));
            }
        }
    }
    let canonical = serde_json::json!({
        "version": 2,
        "root": root.path.display().to_string(),
        "entries": owned,
    });
    let bytes = serde_json::to_vec(&canonical).expect("digest json");
    ContentFingerprint::blake3(&bytes).labeled_hex()
}

pub fn execution_plan_json(plan: &PatchPlan, include_diffs: bool) -> ExecutionPlanJson {
    let entries = plan
        .entries
        .iter()
        .map(|e| match e {
            PlannedChange::Create(c) => ExecutionPlanEntryJson {
                path: c.path.as_str().to_string(),
                operation: "add".into(),
                before_blake3: None,
                after_blake3: Some(c.after_hash.hex()),
                hunks: 0,
                lines_added: c.lines_added,
                lines_deleted: 0,
                unified_diff: if include_diffs {
                    let after = String::from_utf8_lossy(&c.bytes);
                    Some(unified_diff(c.path.as_str(), "", &after))
                } else {
                    None
                },
            },
            PlannedChange::Modify(m) => ExecutionPlanEntryJson {
                path: m.path.as_str().to_string(),
                operation: "update".into(),
                before_blake3: Some(m.before.fingerprint.hex()),
                after_blake3: Some(m.after_hash.hex()),
                hunks: m.hunks,
                lines_added: m.counts.lines_added,
                lines_deleted: m.counts.lines_deleted,
                unified_diff: if include_diffs {
                    let after = String::from_utf8_lossy(&m.after_bytes);
                    Some(unified_diff(m.path.as_str(), &m.before.text, &after))
                } else {
                    None
                },
            },
            PlannedChange::Remove(r) => ExecutionPlanEntryJson {
                path: r.path.as_str().to_string(),
                operation: "delete".into(),
                before_blake3: Some(r.before.fingerprint.hex()),
                after_blake3: None,
                hunks: 0,
                lines_added: 0,
                lines_deleted: r.lines_deleted,
                unified_diff: if include_diffs {
                    Some(unified_diff(r.path.as_str(), &r.before.text, ""))
                } else {
                    None
                },
            },
        })
        .collect();

    ExecutionPlanJson {
        version: 2,
        root: plan.root.path.display().to_string(),
        plan_digest: plan.plan_digest.clone(),
        entries,
        match_evidence: plan.match_evidence.clone(),
    }
}
