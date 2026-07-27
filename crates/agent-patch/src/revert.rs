//! Revert an apply from a self-contained receipt.

use crate::error::{ContentFingerprint, ErrorCode, PublicError};
use crate::journal::{
    new_transaction_id, refuse_if_incomplete, EntryProgress, JournalEntry, JournalState,
    TransactionJournal,
};
use crate::objects::{get_object, object_rel_path, put_object};
use crate::receipt::{self, ApplyReceipt, ReceiptFile};
use crate::root_lock::RootLock;
use serde::Serialize;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Serialize)]
pub struct RevertResult {
    pub root: String,
    pub reverted_transaction_id: String,
    pub new_transaction_id: String,
    pub receipt_path: String,
    pub files: Vec<String>,
}

pub fn revert(root: &Path, receipt_path: &Path) -> Result<RevertResult, PublicError> {
    let _lock = RootLock::acquire(root, Duration::from_secs(30))?;
    refuse_if_incomplete(root)?;

    let receipt = receipt::load(receipt_path)?;
    receipt::validate_objects(root, &receipt)?;
    prove_after_states(root, &receipt)?;

    let txid = new_transaction_id();
    let mut journal_entries = Vec::new();
    let mut inverse_files = Vec::new();

    for f in &receipt.files {
        let (op, before_object, after_blake3, before_blake3) = inverse_meta(root, f)?;
        journal_entries.push(JournalEntry {
            path: f.path.clone(),
            operation: op.clone(),
            before_object: before_object.clone(),
            after_blake3: after_blake3.clone(),
            temp_path: None,
            progress: EntryProgress::Pending,
        });
        inverse_files.push(ReceiptFile {
            path: f.path.clone(),
            operation: op,
            before_blake3,
            after_blake3,
            before_object,
            permissions: f.permissions.clone(),
        });
    }

    let plan_digest = ContentFingerprint::blake3(
        format!("revert:{}:{}", receipt.transaction_id, receipt.plan_digest).as_bytes(),
    )
    .labeled_hex();
    let mut journal = TransactionJournal::new(txid.clone(), plan_digest.clone(), journal_entries);
    journal.write_durable(root)?;
    journal.set_state(JournalState::Committing);
    journal.write_durable(root)?;

    match apply_inverse(root, &receipt) {
        Ok(()) => {
            journal.set_state(JournalState::Completed);
            for e in &mut journal.entries {
                e.progress = EntryProgress::Done;
            }
            journal.write_durable(root)?;

            let new_receipt = ApplyReceipt {
                version: 2,
                root: root.display().to_string(),
                created_at: receipt.created_at.clone(),
                transaction_id: txid.clone(),
                plan_digest,
                tool_contract_version: 2,
                files: inverse_files,
            };
            let path = receipt::write_internal(root, &new_receipt)?;
            Ok(RevertResult {
                root: root.display().to_string(),
                reverted_transaction_id: receipt.transaction_id,
                new_transaction_id: txid,
                receipt_path: path.display().to_string(),
                files: receipt.files.iter().map(|f| f.path.clone()).collect(),
            })
        }
        Err(e) => {
            journal.set_state(JournalState::RollingBack);
            let _ = journal.write_durable(root);
            // Best-effort restore to receipt after-states
            let _ = restore_after_states(root, &receipt);
            journal.set_state(JournalState::RolledBack);
            for entry in &mut journal.entries {
                entry.progress = EntryProgress::RolledBack;
            }
            let _ = journal.write_durable(root);
            Err(e)
        }
    }
}

#[allow(clippy::type_complexity)]
fn inverse_meta(
    root: &Path,
    f: &ReceiptFile,
) -> Result<(String, Option<String>, Option<String>, Option<String>), PublicError> {
    match f.operation.as_str() {
        "add" => {
            // Revert add → delete. Store current (after) as before-image for the delete.
            let abs = root.join(&f.path);
            let bytes = fs::read(&abs).map_err(|e| {
                PublicError::new(
                    ErrorCode::RevertStale,
                    format!("Cannot read {} for revert: {e}", f.path),
                )
            })?;
            let hex = put_object(root, &bytes)?;
            Ok((
                "delete".into(),
                Some(object_rel_path(&hex)),
                None,
                Some(ContentFingerprint::blake3(&bytes).hex()),
            ))
        }
        "update" => {
            // Revert update → write before. Current after becomes before_object of inverse update.
            let abs = root.join(&f.path);
            let after_bytes = fs::read(&abs).map_err(|e| {
                PublicError::new(
                    ErrorCode::RevertStale,
                    format!("Cannot read {} for revert: {e}", f.path),
                )
            })?;
            let after_hex = put_object(root, &after_bytes)?;
            let before_hex = f
                .before_object
                .as_deref()
                .and_then(|r| r.strip_prefix("objects/"))
                .ok_or_else(|| {
                    PublicError::new(ErrorCode::ReceiptInvalid, "Missing before_object")
                })?;
            Ok((
                "update".into(),
                Some(object_rel_path(&after_hex)),
                Some(before_hex.to_string()),
                Some(ContentFingerprint::blake3(&after_bytes).hex()),
            ))
        }
        "delete" => {
            // Revert delete → add from before object
            let before_hex = f
                .before_object
                .as_deref()
                .and_then(|r| r.strip_prefix("objects/"))
                .ok_or_else(|| {
                    PublicError::new(ErrorCode::ReceiptInvalid, "Missing before_object")
                })?;
            let bytes = get_object(root, before_hex)?;
            Ok((
                "add".into(),
                None,
                Some(ContentFingerprint::blake3(&bytes).hex()),
                None,
            ))
        }
        other => Err(PublicError::new(
            ErrorCode::ReceiptInvalid,
            format!("Unknown operation {other}"),
        )),
    }
}

fn prove_after_states(root: &Path, receipt: &ApplyReceipt) -> Result<(), PublicError> {
    for f in &receipt.files {
        let abs = root.join(&f.path);
        match f.operation.as_str() {
            "add" | "update" => {
                let Some(expected) = f.after_blake3.as_deref() else {
                    return Err(PublicError::new(
                        ErrorCode::ReceiptInvalid,
                        format!("Missing after_blake3 for {}", f.path),
                    ));
                };
                if !abs.is_file() {
                    return Err(PublicError::new(
                        ErrorCode::RevertStale,
                        format!("{} missing; cannot revert", f.path),
                    ));
                }
                let bytes = fs::read(&abs).map_err(|e| {
                    PublicError::new(ErrorCode::IoError, format!("Cannot read {}: {e}", f.path))
                })?;
                if ContentFingerprint::blake3(&bytes).hex() != expected {
                    return Err(PublicError::new(
                        ErrorCode::RevertStale,
                        format!("{} no longer matches receipt after-state", f.path),
                    ));
                }
            }
            "delete" => {
                if abs.exists() {
                    return Err(PublicError::new(
                        ErrorCode::RevertStale,
                        format!("{} exists but receipt expected delete", f.path),
                    ));
                }
            }
            other => {
                return Err(PublicError::new(
                    ErrorCode::ReceiptInvalid,
                    format!("Unknown operation {other}"),
                ));
            }
        }
    }
    Ok(())
}

fn apply_inverse(root: &Path, receipt: &ApplyReceipt) -> Result<(), PublicError> {
    for f in receipt.files.iter().rev() {
        let abs = root.join(&f.path);
        match f.operation.as_str() {
            "add" => {
                fs::remove_file(&abs).map_err(|e| {
                    PublicError::new(
                        ErrorCode::AtomicCommitFailed,
                        format!("Cannot remove {} during revert: {e}", f.path),
                    )
                })?;
            }
            "update" | "delete" => {
                let hex = f
                    .before_object
                    .as_deref()
                    .and_then(|r| r.strip_prefix("objects/"))
                    .ok_or_else(|| {
                        PublicError::new(ErrorCode::ReceiptObjectMissing, "Missing before_object")
                    })?;
                let bytes = get_object(root, hex)?;
                let mode = match &f.permissions {
                    Some(p) => match p.mode {
                        Some(m) => m,
                        None if p.executable => 0o755,
                        None => 0o644,
                    },
                    None => 0o644,
                };
                write_bytes_atomic(&abs, &bytes, mode)?;
            }
            other => {
                return Err(PublicError::new(
                    ErrorCode::ReceiptInvalid,
                    format!("Unknown operation {other}"),
                ));
            }
        }
    }
    Ok(())
}

fn restore_after_states(root: &Path, receipt: &ApplyReceipt) -> Result<(), PublicError> {
    // Best-effort: for update/add write after content is not stored as objects.
    // We only restore deletes (re-delete) and cannot recreate after bytes without CAS.
    // Leave as best-effort remove of restored deletes only when after was delete.
    for f in &receipt.files {
        if f.operation == "delete" {
            let abs = root.join(&f.path);
            if abs.exists() {
                let _ = fs::remove_file(&abs);
            }
        }
    }
    Ok(())
}

fn write_bytes_atomic(dest: &Path, bytes: &[u8], mode: u32) -> Result<(), PublicError> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            PublicError::new(
                ErrorCode::AtomicCommitFailed,
                format!("Cannot create parent: {e}"),
            )
        })?;
    }
    let tmp = dest.with_extension("agent-patch.revert.tmp");
    {
        let mut f = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp)
            .map_err(|e| PublicError::new(ErrorCode::AtomicCommitFailed, format!("temp: {e}")))?;
        f.write_all(bytes)
            .map_err(|e| PublicError::new(ErrorCode::AtomicCommitFailed, format!("write: {e}")))?;
        f.sync_all()
            .map_err(|e| PublicError::new(ErrorCode::AtomicCommitFailed, format!("fsync: {e}")))?;
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
            ErrorCode::AtomicCommitFailed,
            format!("persist {}: {e}", dest.display()),
        )
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
    use crate::objects::put_object;

    #[test]
    fn revert_update_restores_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let before = b"before\n";
        let after = b"after\n";
        fs::write(root.join("f.txt"), after).unwrap();
        let hex = put_object(root, before).unwrap();
        let receipt = ApplyReceipt {
            version: 2,
            root: root.display().to_string(),
            created_at: "0".into(),
            transaction_id: "orig".into(),
            plan_digest: "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .into(),
            tool_contract_version: 2,
            files: vec![ReceiptFile {
                path: "f.txt".into(),
                operation: "update".into(),
                before_blake3: Some(ContentFingerprint::blake3(before).hex()),
                after_blake3: Some(ContentFingerprint::blake3(after).hex()),
                before_object: Some(object_rel_path(&hex)),
                permissions: None,
            }],
        };
        let path = receipt::write_internal(root, &receipt).unwrap();
        revert(root, &path).unwrap();
        assert_eq!(fs::read(root.join("f.txt")).unwrap(), before);
    }
}
