//! Patch planner — pairs operations with applied in-memory results.

use crate::engine::{apply_update, DiffCounts};
use crate::error::{ContentFingerprint, ErrorCode, NewlineStyle, PublicError};
use crate::path_policy::{CanonicalRoot, RepoPath};
use crate::protocol::ast::FileOperation;
use crate::snapshot::{FileSnapshot, FileState, PresentFile};
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct PatchPlan {
    pub root: CanonicalRoot,
    pub entries: Vec<PlannedChange>,
    pub base_fingerprints: BTreeMap<RepoPath, Option<ContentFingerprint>>,
    pub summary: PlanSummary,
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
}

#[derive(Debug, Clone)]
pub struct PlannedRemove {
    pub path: RepoPath,
    pub abs_path: std::path::PathBuf,
    pub before: PresentFile,
    pub operation_index: usize,
    pub lines_deleted: usize,
}

pub fn build_plan(
    root: CanonicalRoot,
    ops: &[(usize, RepoPath, &FileOperation)],
    snapshots: &BTreeMap<RepoPath, FileSnapshot>,
) -> Result<PatchPlan, PublicError> {
    let mut entries = Vec::new();
    let mut base_fingerprints = BTreeMap::new();
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

                // Preserve BOM: if base starts with BOM, ensure result keeps it
                let applied = apply_update(&present.text, update, newline, present.final_newline)
                    .map_err(|e| e.with_operation(*index))?;

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

    // Deterministic lexicographic order by path
    entries.sort_by(|a, b| a.path().cmp(b.path()));

    Ok(PatchPlan {
        root,
        entries,
        base_fingerprints,
        summary,
    })
}
