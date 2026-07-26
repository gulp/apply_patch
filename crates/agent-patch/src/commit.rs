//! Commit coordinator with revalidation and rollback.

use crate::error::{ErrorCode, Limits, PublicError};
use crate::fs::{FileSystem, TempHandle};
use crate::plan::{PatchPlan, PlannedChange};
use crate::snapshot::current_identity;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct CommitResult {
    pub committed: Vec<CommittedEntry>,
}

#[derive(Debug)]
pub struct CommittedEntry {
    pub path: String,
    pub operation: &'static str,
}

struct Prepared {
    change_index: usize,
    temp: TempHandle,
    mode: Option<u32>,
    created_dirs: Vec<PathBuf>,
}

enum CommitAction {
    Modified {
        path: PathBuf,
        rollback_bytes: Vec<u8>,
        mode: u32,
    },
    Created {
        path: PathBuf,
        created_dirs: Vec<PathBuf>,
    },
    Removed {
        path: PathBuf,
        rollback_bytes: Vec<u8>,
        mode: u32,
    },
}

pub fn commit_plan(
    fs: &dyn FileSystem,
    plan: &PatchPlan,
    limits: &Limits,
) -> Result<CommitResult, PublicError> {
    revalidate(fs, plan, limits)?;

    let mut prepared: Vec<Prepared> = Vec::new();

    for (idx, entry) in plan.entries.iter().enumerate() {
        match entry {
            PlannedChange::Create(c) => {
                let parent = c
                    .abs_path
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| PathBuf::from("."));
                let created_dirs = match ensure_parents(fs, &plan.root.path, &parent) {
                    Ok(d) => d,
                    Err(e) => {
                        drop_prepared(fs, prepared);
                        return Err(e);
                    }
                };
                let mut temp = match fs.create_temp_near(&c.abs_path) {
                    Ok(t) => t,
                    Err(e) => {
                        drop_prepared(fs, prepared);
                        return Err(PublicError::new(
                            ErrorCode::AtomicCommitFailed,
                            format!("Cannot create temp for {}: {e}", c.path),
                        )
                        .with_path(c.path.as_str()));
                    }
                };
                if let Err(e) = fs.write_temp(&mut temp, &c.bytes) {
                    let _ = fs.remove_file(&temp.path);
                    drop_prepared(fs, prepared);
                    return Err(PublicError::new(
                        ErrorCode::AtomicCommitFailed,
                        format!("Cannot write temp for {}: {e}", c.path),
                    )
                    .with_path(c.path.as_str()));
                }
                prepared.push(Prepared {
                    change_index: idx,
                    temp,
                    mode: Some(0o644),
                    created_dirs,
                });
            }
            PlannedChange::Modify(m) => {
                let mut temp = match fs.create_temp_near(&m.abs_path) {
                    Ok(t) => t,
                    Err(e) => {
                        drop_prepared(fs, prepared);
                        return Err(PublicError::new(
                            ErrorCode::AtomicCommitFailed,
                            format!("Cannot create temp for {}: {e}", m.path),
                        )
                        .with_path(m.path.as_str()));
                    }
                };
                if let Err(e) = fs.write_temp(&mut temp, &m.after_bytes) {
                    let _ = fs.remove_file(&temp.path);
                    drop_prepared(fs, prepared);
                    return Err(PublicError::new(
                        ErrorCode::AtomicCommitFailed,
                        format!("Cannot write temp for {}: {e}", m.path),
                    )
                    .with_path(m.path.as_str()));
                }
                let _ = fs.set_permissions(&temp.path, m.before.permissions);
                prepared.push(Prepared {
                    change_index: idx,
                    temp,
                    mode: Some(m.before.permissions),
                    created_dirs: Vec::new(),
                });
            }
            PlannedChange::Remove(_) => {}
        }
    }

    let mut committed_ops: Vec<CommitAction> = Vec::new();

    for (idx, entry) in plan.entries.iter().enumerate() {
        let action = match entry {
            PlannedChange::Modify(m) => {
                let prep_pos = prepared.iter().position(|p| p.change_index == idx);
                let Some(pos) = prep_pos else {
                    let _ = rollback(fs, &committed_ops);
                    drop_prepared(fs, prepared);
                    return Err(PublicError::new(
                        ErrorCode::InternalError,
                        "Missing prepared temp.",
                    ));
                };
                let prep = prepared.swap_remove(pos);
                let temp_path = prep.temp.path.clone();
                // Drop NamedTempFile without deleting by persisting via rename
                std::mem::forget(prep.temp);
                match fs.rename(&temp_path, &m.abs_path) {
                    Ok(()) => CommitAction::Modified {
                        path: m.abs_path.clone(),
                        rollback_bytes: m.before.bytes.clone(),
                        mode: m.before.permissions,
                    },
                    Err(e) => {
                        let _ = fs.remove_file(&temp_path);
                        let _ = rollback(fs, &committed_ops);
                        drop_prepared(fs, prepared);
                        return Err(PublicError::new(
                            ErrorCode::AtomicCommitFailed,
                            format!("Rename failed for {}: {e}", m.path),
                        )
                        .with_path(m.path.as_str()));
                    }
                }
            }
            PlannedChange::Create(c) => {
                let prep_pos = prepared.iter().position(|p| p.change_index == idx);
                let Some(pos) = prep_pos else {
                    let _ = rollback(fs, &committed_ops);
                    drop_prepared(fs, prepared);
                    return Err(PublicError::new(
                        ErrorCode::InternalError,
                        "Missing prepared temp.",
                    ));
                };
                let prep = prepared.swap_remove(pos);
                let temp_path = prep.temp.path.clone();
                let created_dirs = prep.created_dirs.clone();
                let mode = prep.mode;
                std::mem::forget(prep.temp);
                match fs.rename(&temp_path, &c.abs_path) {
                    Ok(()) => {
                        if let Some(mode) = mode {
                            let _ = fs.set_permissions(&c.abs_path, mode);
                        }
                        CommitAction::Created {
                            path: c.abs_path.clone(),
                            created_dirs,
                        }
                    }
                    Err(e) => {
                        let _ = fs.remove_file(&temp_path);
                        let _ = rollback(fs, &committed_ops);
                        drop_prepared(fs, prepared);
                        return Err(PublicError::new(
                            ErrorCode::AtomicCommitFailed,
                            format!("Rename failed for {}: {e}", c.path),
                        )
                        .with_path(c.path.as_str()));
                    }
                }
            }
            PlannedChange::Remove(r) => match fs.remove_file(&r.abs_path) {
                Ok(()) => CommitAction::Removed {
                    path: r.abs_path.clone(),
                    rollback_bytes: r.before.bytes.clone(),
                    mode: r.before.permissions,
                },
                Err(e) => {
                    let _ = rollback(fs, &committed_ops);
                    drop_prepared(fs, prepared);
                    return Err(PublicError::new(
                        ErrorCode::AtomicCommitFailed,
                        format!("Delete failed for {}: {e}", r.path),
                    )
                    .with_path(r.path.as_str()));
                }
            },
        };
        committed_ops.push(action);
    }

    drop_prepared(fs, prepared);

    let committed = plan
        .entries
        .iter()
        .map(|entry| CommittedEntry {
            path: entry.path().as_str().to_string(),
            operation: match entry {
                PlannedChange::Create(_) => "add",
                PlannedChange::Modify(_) => "update",
                PlannedChange::Remove(_) => "delete",
            },
        })
        .collect();

    Ok(CommitResult { committed })
}

fn revalidate(fs: &dyn FileSystem, plan: &PatchPlan, limits: &Limits) -> Result<(), PublicError> {
    for entry in &plan.entries {
        let expected = plan.base_fingerprints.get(entry.path());
        let (exists, hash) = current_identity(fs, entry.abs_path(), limits)?;
        match expected {
            Some(None) => {
                if exists {
                    return Err(PublicError::new(
                        ErrorCode::ConcurrentModification,
                        format!("Path {} appeared before commit.", entry.path()),
                    )
                    .with_path(entry.path().as_str()));
                }
            }
            Some(Some(expected_hash)) => {
                if !exists {
                    return Err(PublicError::new(
                        ErrorCode::ConcurrentModification,
                        format!("Path {} disappeared before commit.", entry.path()),
                    )
                    .with_path(entry.path().as_str()));
                }
                if hash.as_ref() != Some(expected_hash) {
                    return Err(PublicError::new(
                        ErrorCode::ConcurrentModification,
                        format!("Path {} changed before commit.", entry.path()),
                    )
                    .with_path(entry.path().as_str()));
                }
            }
            None => {
                return Err(PublicError::new(
                    ErrorCode::InternalError,
                    "Missing base fingerprint.",
                ));
            }
        }
    }
    Ok(())
}

fn drop_prepared(fs: &dyn FileSystem, prepared: Vec<Prepared>) {
    for p in prepared {
        let path = p.temp.path.clone();
        drop(p.temp);
        let _ = fs.remove_file(&path);
        for dir in p.created_dirs.iter().rev() {
            let _ = std::fs::remove_dir(dir);
        }
    }
}

fn rollback(fs: &dyn FileSystem, committed: &[CommitAction]) -> Result<(), PublicError> {
    let mut failed_paths = Vec::new();
    for action in committed.iter().rev() {
        match action {
            CommitAction::Modified {
                path,
                rollback_bytes,
                mode,
            } => {
                if write_bytes_atomic(fs, path, rollback_bytes, *mode).is_err() {
                    failed_paths.push(path.display().to_string());
                }
            }
            CommitAction::Created { path, created_dirs } => {
                if fs.remove_file(path).is_err() {
                    failed_paths.push(path.display().to_string());
                }
                for dir in created_dirs.iter().rev() {
                    let _ = std::fs::remove_dir(dir);
                }
            }
            CommitAction::Removed {
                path,
                rollback_bytes,
                mode,
            } => {
                if write_bytes_atomic(fs, path, rollback_bytes, *mode).is_err() {
                    failed_paths.push(path.display().to_string());
                }
            }
        }
    }
    if !failed_paths.is_empty() {
        return Err(PublicError::new(
            ErrorCode::RollbackFailed,
            format!("Rollback incomplete for: {}", failed_paths.join(", ")),
        ));
    }
    Ok(())
}

fn write_bytes_atomic(
    fs: &dyn FileSystem,
    dest: &Path,
    bytes: &[u8],
    mode: u32,
) -> Result<(), PublicError> {
    let mut temp = fs
        .create_temp_near(dest)
        .map_err(|e| PublicError::new(ErrorCode::RollbackFailed, format!("temp: {e}")))?;
    fs.write_temp(&mut temp, bytes)
        .map_err(|e| PublicError::new(ErrorCode::RollbackFailed, format!("write: {e}")))?;
    let _ = fs.set_permissions(&temp.path, mode);
    fs.persist_temp(temp, dest)
        .map_err(|e| PublicError::new(ErrorCode::RollbackFailed, format!("persist: {e}")))?;
    Ok(())
}

fn ensure_parents(
    fs: &dyn FileSystem,
    root: &Path,
    parent: &Path,
) -> Result<Vec<PathBuf>, PublicError> {
    let mut created = Vec::new();
    if parent.exists() {
        return Ok(created);
    }
    let mut stack = Vec::new();
    let mut cur = parent.to_path_buf();
    while !cur.exists() && cur.starts_with(root) {
        stack.push(cur.clone());
        match cur.parent() {
            Some(p) if p != cur => cur = p.to_path_buf(),
            _ => break,
        }
    }
    for dir in stack.into_iter().rev() {
        if !dir.exists() {
            fs.create_dir_all(&dir).map_err(|e| {
                PublicError::new(
                    ErrorCode::AtomicCommitFailed,
                    format!("Cannot create directory {}: {e}", dir.display()),
                )
            })?;
            created.push(dir);
        }
    }
    Ok(created)
}
