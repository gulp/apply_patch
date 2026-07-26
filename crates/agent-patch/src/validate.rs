//! Semantic validation of parsed patches against snapshots and limits.

use crate::error::{ErrorCode, Limits, NewlineStyle, PublicError};
use crate::path_policy::{parse_repo_path, RepoPath};
use crate::protocol::ast::{FileOperation, PatchDocument};
use crate::snapshot::{FileSnapshot, FileState};
use std::collections::{BTreeMap, BTreeSet};

pub struct ValidatedPatch {
    pub operations: Vec<ValidatedOp>,
}

pub struct ValidatedOp {
    pub index: usize,
    pub repo_path: RepoPath,
    pub operation: FileOperation,
}

pub fn validate_document<'a>(
    doc: &'a PatchDocument,
    limits: &Limits,
) -> Result<Vec<(usize, RepoPath, &'a FileOperation)>, PublicError> {
    if doc.operations.len() > limits.max_files {
        return Err(PublicError::new(
            ErrorCode::LimitFileCount,
            format!(
                "Patch affects {} files; max-files is {}.",
                doc.operations.len(),
                limits.max_files
            ),
        ));
    }

    let mut total_hunks = 0usize;
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut out = Vec::new();

    for (index, op) in doc.operations.iter().enumerate() {
        let raw = op.path();
        let repo_path = parse_repo_path(raw).map_err(|e| e.with_operation(index))?;
        if !seen.insert(repo_path.as_str().to_string()) {
            return Err(PublicError::new(
                ErrorCode::DuplicatePath,
                format!("Duplicate operation on path {}.", repo_path),
            )
            .with_path(repo_path.as_str())
            .with_operation(index)
            .with_source(op.source_span()));
        }

        if let FileOperation::Update(u) = op {
            if u.hunks.len() > limits.max_hunks_per_file {
                return Err(PublicError::new(
                    ErrorCode::LimitHunkCount,
                    format!(
                        "File {} has {} hunks; max-hunks-per-file is {}.",
                        repo_path,
                        u.hunks.len(),
                        limits.max_hunks_per_file
                    ),
                )
                .with_path(repo_path.as_str())
                .with_operation(index));
            }
            total_hunks += u.hunks.len();
        }

        out.push((index, repo_path, op));
    }

    if total_hunks > limits.max_total_hunks {
        return Err(PublicError::new(
            ErrorCode::LimitHunkCount,
            format!(
                "Patch has {} total hunks; max-total-hunks is {}.",
                total_hunks, limits.max_total_hunks
            ),
        ));
    }

    Ok(out)
}

pub fn validate_against_snapshots(
    ops: &[(usize, RepoPath, &FileOperation)],
    snapshots: &BTreeMap<RepoPath, FileSnapshot>,
) -> Result<(), PublicError> {
    for (index, repo_path, op) in ops {
        let snap = snapshots.get(repo_path).ok_or_else(|| {
            PublicError::new(ErrorCode::InternalError, "Missing snapshot for path.")
                .with_path(repo_path.as_str())
        })?;

        match (*op, &snap.state) {
            (FileOperation::Add(_), FileState::Present(_)) => {
                return Err(PublicError::new(
                    ErrorCode::FileAlreadyExists,
                    format!("Add File target {} already exists.", repo_path),
                )
                .with_path(repo_path.as_str())
                .with_operation(*index)
                .with_source(op.source_span()));
            }
            (FileOperation::Add(_), FileState::Missing) => {}
            (FileOperation::Update(_), FileState::Missing) => {
                return Err(PublicError::new(
                    ErrorCode::FileNotFound,
                    format!("Update File target {} does not exist.", repo_path),
                )
                .with_path(repo_path.as_str())
                .with_operation(*index)
                .with_source(op.source_span()));
            }
            (FileOperation::Update(_), FileState::Present(p)) => {
                if p.newline_style == NewlineStyle::Mixed {
                    return Err(PublicError::new(
                        ErrorCode::MixedLineEndings,
                        format!("Update File target {} has mixed line endings.", repo_path),
                    )
                    .with_path(repo_path.as_str())
                    .with_operation(*index));
                }
            }
            (FileOperation::Delete(_), FileState::Missing) => {
                return Err(PublicError::new(
                    ErrorCode::FileNotFound,
                    format!("Delete File target {} does not exist.", repo_path),
                )
                .with_path(repo_path.as_str())
                .with_operation(*index)
                .with_source(op.source_span()));
            }
            (FileOperation::Delete(_), FileState::Present(_)) => {}
        }
    }
    Ok(())
}
