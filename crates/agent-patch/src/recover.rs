//! Crash recovery from durable transaction journals.

use crate::error::{ContentFingerprint, ErrorCode, PublicError};
use crate::journal::{
    list_incomplete, EntryProgress, JournalEntry, JournalState, TransactionJournal,
};
use crate::objects::get_object;
use crate::root_lock::RootLock;
use serde::Serialize;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Serialize)]
pub struct RecoverResult {
    pub root: String,
    pub recovered: Vec<RecoveredTx>,
}

#[derive(Debug, Serialize)]
pub struct RecoveredTx {
    pub transaction_id: String,
    pub outcome: String,
    pub state: String,
}

pub fn recover(root: &Path, transaction: Option<&str>) -> Result<RecoverResult, PublicError> {
    let _lock = RootLock::acquire(root, Duration::from_secs(30))?;
    let targets = match transaction {
        Some(txid) => vec![txid.to_string()],
        None => list_incomplete(root)?,
    };

    if targets.is_empty() {
        return Ok(RecoverResult {
            root: root.display().to_string(),
            recovered: Vec::new(),
        });
    }

    let mut recovered = Vec::new();
    for txid in targets {
        let outcome = recover_one(root, &txid)?;
        recovered.push(outcome);
    }
    Ok(RecoverResult {
        root: root.display().to_string(),
        recovered,
    })
}

fn recover_one(root: &Path, txid: &str) -> Result<RecoveredTx, PublicError> {
    let mut journal = TransactionJournal::load(root, txid)?;
    match journal.state {
        JournalState::Completed | JournalState::RolledBack => {
            return Ok(RecoveredTx {
                transaction_id: txid.to_string(),
                outcome: "noop".into(),
                state: format!("{:?}", journal.state),
            });
        }
        JournalState::Prepared => {
            // No visible mutations should have started; mark rolled back.
            journal.set_state(JournalState::RolledBack);
            for e in &mut journal.entries {
                e.progress = EntryProgress::RolledBack;
            }
            journal.write_durable(root)?;
            return Ok(RecoveredTx {
                transaction_id: txid.to_string(),
                outcome: "rolled_back".into(),
                state: "ROLLED_BACK".into(),
            });
        }
        JournalState::Committing | JournalState::RollingBack => {}
    }

    match all_after_match(root, &journal)? {
        AfterCheck::AllAfter => {
            journal.set_state(JournalState::Completed);
            for e in &mut journal.entries {
                e.progress = EntryProgress::Done;
            }
            journal.write_durable(root)?;
            Ok(RecoveredTx {
                transaction_id: txid.to_string(),
                outcome: "completed".into(),
                state: "COMPLETED".into(),
            })
        }
        AfterCheck::NeedsRollback => {
            journal.set_state(JournalState::RollingBack);
            journal.write_durable(root)?;
            restore_all_before(root, &journal.entries)?;
            journal.set_state(JournalState::RolledBack);
            for e in &mut journal.entries {
                e.progress = EntryProgress::RolledBack;
            }
            journal.write_durable(root)?;
            Ok(RecoveredTx {
                transaction_id: txid.to_string(),
                outcome: "rolled_back".into(),
                state: "ROLLED_BACK".into(),
            })
        }
        AfterCheck::Ambiguous => Err(PublicError::new(
            ErrorCode::RecoveryAmbiguous,
            format!(
                "Transaction {txid} has an ambiguous mix of before/after states; manual inspection required."
            ),
        )
        .with_hint(format!(
            "Inspect .agent-patch/transactions/{txid}/journal.json and objects; do not invent mixed state."
        ))
        .with_recovery_required(true)),
    }
}

enum AfterCheck {
    AllAfter,
    NeedsRollback,
    Ambiguous,
}

fn all_after_match(root: &Path, journal: &TransactionJournal) -> Result<AfterCheck, PublicError> {
    let mut any_after = false;
    let mut any_before_or_other = false;
    for entry in &journal.entries {
        let abs = root.join(&entry.path);
        match entry.operation.as_str() {
            "add" | "update" => {
                let Some(expected) = entry.after_blake3.as_deref() else {
                    return Ok(AfterCheck::Ambiguous);
                };
                if !abs.is_file() {
                    any_before_or_other = true;
                    continue;
                }
                let bytes = fs::read(&abs).map_err(|e| {
                    PublicError::new(
                        ErrorCode::IoError,
                        format!("Cannot read {} during recovery: {e}", entry.path),
                    )
                })?;
                if ContentFingerprint::blake3(&bytes).hex() == expected {
                    any_after = true;
                } else if matches_before(root, entry, &bytes)? {
                    any_before_or_other = true;
                } else {
                    return Ok(AfterCheck::Ambiguous);
                }
            }
            "delete" => {
                if abs.exists() {
                    if let Some(rel) = entry.before_object.as_deref() {
                        let bytes = fs::read(&abs).map_err(|e| {
                            PublicError::new(
                                ErrorCode::IoError,
                                format!("Cannot read {} during recovery: {e}", entry.path),
                            )
                        })?;
                        if matches_before_bytes(root, rel, &bytes)? {
                            any_before_or_other = true;
                        } else {
                            return Ok(AfterCheck::Ambiguous);
                        }
                    } else {
                        return Ok(AfterCheck::Ambiguous);
                    }
                } else {
                    any_after = true;
                }
            }
            _ => return Ok(AfterCheck::Ambiguous),
        }
    }
    if any_after && !any_before_or_other {
        Ok(AfterCheck::AllAfter)
    } else if !any_after {
        Ok(AfterCheck::NeedsRollback)
    } else {
        // Mixed after + before across entries — never invent mixed roll-forward.
        Ok(AfterCheck::Ambiguous)
    }
}

fn matches_before(root: &Path, entry: &JournalEntry, bytes: &[u8]) -> Result<bool, PublicError> {
    let Some(rel) = entry.before_object.as_deref() else {
        return Ok(false);
    };
    matches_before_bytes(root, rel, bytes)
}

fn matches_before_bytes(root: &Path, rel: &str, bytes: &[u8]) -> Result<bool, PublicError> {
    let Some(hex) = rel.strip_prefix("objects/") else {
        return Ok(false);
    };
    let before = get_object(root, hex)?;
    Ok(before == bytes)
}

fn restore_all_before(root: &Path, entries: &[JournalEntry]) -> Result<(), PublicError> {
    for entry in entries.iter().rev() {
        let abs = root.join(&entry.path);
        match entry.operation.as_str() {
            "add" => {
                if abs.exists() {
                    fs::remove_file(&abs).map_err(|e| {
                        PublicError::new(
                            ErrorCode::RollbackFailed,
                            format!("Cannot remove {} during recover: {e}", entry.path),
                        )
                        .with_recovery_required(true)
                    })?;
                }
            }
            "update" | "delete" => {
                let rel = entry.before_object.as_deref().ok_or_else(|| {
                    PublicError::new(
                        ErrorCode::RecoveryAmbiguous,
                        format!("Missing before_object for {}", entry.path),
                    )
                    .with_recovery_required(true)
                })?;
                let hex = rel.strip_prefix("objects/").ok_or_else(|| {
                    PublicError::new(
                        ErrorCode::RecoveryAmbiguous,
                        format!("Invalid before_object path for {}", entry.path),
                    )
                    .with_recovery_required(true)
                })?;
                let bytes = get_object(root, hex)?;
                write_bytes_atomic(&abs, &bytes, 0o644)?;
            }
            other => {
                return Err(PublicError::new(
                    ErrorCode::RecoveryAmbiguous,
                    format!("Unknown operation {other} in journal"),
                )
                .with_recovery_required(true));
            }
        }
    }
    Ok(())
}

fn write_bytes_atomic(dest: &Path, bytes: &[u8], mode: u32) -> Result<(), PublicError> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            PublicError::new(
                ErrorCode::RollbackFailed,
                format!("Cannot create parent for {}: {e}", dest.display()),
            )
        })?;
    }
    let tmp = dest.with_extension("agent-patch.tmp");
    {
        let mut f = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp)
            .map_err(|e| {
                PublicError::new(
                    ErrorCode::RollbackFailed,
                    format!("Cannot create temp {}: {e}", tmp.display()),
                )
            })?;
        f.write_all(bytes).map_err(|e| {
            PublicError::new(ErrorCode::RollbackFailed, format!("Cannot write temp: {e}"))
        })?;
        f.sync_all().map_err(|e| {
            PublicError::new(ErrorCode::RollbackFailed, format!("Cannot fsync temp: {e}"))
        })?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(mode));
    }
    #[cfg(not(unix))]
    {
        let _ = mode;
    }
    fs::rename(&tmp, dest).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        PublicError::new(
            ErrorCode::RollbackFailed,
            format!("Cannot persist {}: {e}", dest.display()),
        )
        .with_recovery_required(true)
    })?;
    if let Some(parent) = dest.parent() {
        if let Ok(d) = File::open(parent) {
            let _ = d.sync_all();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::{object_rel_path, put_object};

    #[test]
    fn recover_prepared_marks_rolled_back() {
        let dir = tempfile::tempdir().unwrap();
        let j = TransactionJournal::new("txp".into(), "blake3:x".into(), vec![]);
        j.write_durable(dir.path()).unwrap();
        let r = recover(dir.path(), Some("txp")).unwrap();
        assert_eq!(r.recovered[0].outcome, "rolled_back");
        assert!(list_incomplete(dir.path()).unwrap().is_empty());
    }

    #[test]
    fn recover_committing_restores_before() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let path = root.join("f.txt");
        let before = b"before\n";
        let after = b"after\n";
        fs::write(&path, after).unwrap();
        let hex = put_object(root, before).unwrap();
        let mut j = TransactionJournal::new(
            "txc".into(),
            "blake3:p".into(),
            vec![JournalEntry {
                path: "f.txt".into(),
                operation: "update".into(),
                before_object: Some(object_rel_path(&hex)),
                after_blake3: Some(ContentFingerprint::blake3(after).hex()),
                temp_path: None,
                progress: EntryProgress::Visible,
            }],
        );
        j.set_state(JournalState::Committing);
        j.write_durable(root).unwrap();

        // Not all-after if we change expected? File is after — should complete.
        let r = recover(root, Some("txc")).unwrap();
        assert_eq!(r.recovered[0].outcome, "completed");
        assert_eq!(fs::read(&path).unwrap(), after);

        // Fresh incomplete with wrong content → rollback
        fs::write(&path, b"weird\n").unwrap();
        let mut j2 = TransactionJournal::new(
            "txc2".into(),
            "blake3:p".into(),
            vec![JournalEntry {
                path: "f.txt".into(),
                operation: "update".into(),
                before_object: Some(object_rel_path(&hex)),
                after_blake3: Some(ContentFingerprint::blake3(after).hex()),
                temp_path: None,
                progress: EntryProgress::Visible,
            }],
        );
        j2.set_state(JournalState::Committing);
        j2.write_durable(root).unwrap();
        let err = recover(root, Some("txc2")).unwrap_err();
        assert_eq!(err.code, ErrorCode::RecoveryAmbiguous);
    }

    #[test]
    fn recover_partial_before_rolls_back() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let path = root.join("g.txt");
        let before = b"old\n";
        fs::write(&path, before).unwrap();
        let hex = put_object(root, before).unwrap();
        let after = b"new\n";
        let mut j = TransactionJournal::new(
            "txb".into(),
            "blake3:p".into(),
            vec![JournalEntry {
                path: "g.txt".into(),
                operation: "update".into(),
                before_object: Some(object_rel_path(&hex)),
                after_blake3: Some(ContentFingerprint::blake3(after).hex()),
                temp_path: None,
                progress: EntryProgress::Pending,
            }],
        );
        j.set_state(JournalState::Committing);
        j.write_durable(root).unwrap();
        let r = recover(root, Some("txb")).unwrap();
        assert_eq!(r.recovered[0].outcome, "rolled_back");
        assert_eq!(fs::read(&path).unwrap(), before);
    }
}
