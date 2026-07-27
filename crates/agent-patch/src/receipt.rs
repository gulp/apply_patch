//! Apply receipts — self-contained before-image references.

use crate::error::{ErrorCode, PublicError};
use crate::journal::JournalEntry;
use crate::objects::object_path;
use crate::plan::{PatchPlan, PlannedChange};
use crate::store_layout::{ensure_layout, receipts_dir};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyReceipt {
    pub version: u32,
    pub root: String,
    pub created_at: String,
    pub transaction_id: String,
    pub plan_digest: String,
    pub tool_contract_version: u32,
    pub files: Vec<ReceiptFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiptFile {
    pub path: String,
    pub operation: String,
    pub before_blake3: Option<String>,
    pub after_blake3: Option<String>,
    pub before_object: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<ReceiptPermissions>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiptPermissions {
    pub executable: bool,
    /// Unix mode bits when captured (e.g. 0o644). Optional for older receipts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<u32>,
}

pub fn build_receipt(
    plan: &PatchPlan,
    transaction_id: &str,
    journal_entries: &[JournalEntry],
) -> Result<ApplyReceipt, PublicError> {
    let mut files = Vec::new();
    for (entry, je) in plan.entries.iter().zip(journal_entries.iter()) {
        let (before_blake3, after_blake3, permissions) = match entry {
            PlannedChange::Create(c) => (None, Some(c.after_hash.hex()), None),
            PlannedChange::Modify(m) => (
                Some(m.before.fingerprint.hex()),
                Some(m.after_hash.hex()),
                Some(ReceiptPermissions {
                    executable: m.before.permissions & 0o111 != 0,
                    mode: Some(m.before.permissions),
                }),
            ),
            PlannedChange::Remove(r) => (
                Some(r.before.fingerprint.hex()),
                None,
                Some(ReceiptPermissions {
                    executable: r.before.permissions & 0o111 != 0,
                    mode: Some(r.before.permissions),
                }),
            ),
        };
        let before_object = je.before_object.clone();
        if matches!(entry, PlannedChange::Modify(_) | PlannedChange::Remove(_))
            && before_object.is_none()
        {
            return Err(PublicError::new(
                ErrorCode::InternalError,
                format!("Receipt for {} missing before_object", entry.path()),
            ));
        }
        files.push(ReceiptFile {
            path: entry.path().as_str().to_string(),
            operation: je.operation.clone(),
            before_blake3,
            after_blake3,
            before_object,
            permissions,
        });
    }
    Ok(ApplyReceipt {
        version: 2,
        root: plan.root.path.display().to_string(),
        created_at: now_secs(),
        transaction_id: transaction_id.to_string(),
        plan_digest: plan.plan_digest.clone(),
        tool_contract_version: 2,
        files,
    })
}

pub fn write_internal(root: &Path, receipt: &ApplyReceipt) -> Result<PathBuf, PublicError> {
    ensure_layout(root)?;
    validate_objects(root, receipt)?;
    let path = receipts_dir(root).join(format!("{}.json", receipt.transaction_id));
    write_atomic(&path, receipt)?;
    Ok(path)
}

pub fn export_copy(receipt: &ApplyReceipt, dest: &Path) -> Result<(), PublicError> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            PublicError::new(
                ErrorCode::IoError,
                format!("Cannot create receipt parent: {e}"),
            )
        })?;
    }
    write_atomic(dest, receipt)
}

pub fn load(path: &Path) -> Result<ApplyReceipt, PublicError> {
    let bytes = fs::read(path).map_err(|e| {
        PublicError::new(
            ErrorCode::ReceiptInvalid,
            format!("Cannot read receipt {}: {e}", path.display()),
        )
    })?;
    let receipt: ApplyReceipt = serde_json::from_slice(&bytes).map_err(|e| {
        PublicError::new(
            ErrorCode::ReceiptInvalid,
            format!("Corrupt receipt {}: {e}", path.display()),
        )
    })?;
    if receipt.version != 2 || receipt.tool_contract_version != 2 {
        return Err(PublicError::new(
            ErrorCode::ReceiptInvalid,
            format!(
                "Unsupported receipt version {} / contract {}",
                receipt.version, receipt.tool_contract_version
            ),
        ));
    }
    Ok(receipt)
}

pub fn validate_objects(root: &Path, receipt: &ApplyReceipt) -> Result<(), PublicError> {
    for f in &receipt.files {
        if matches!(f.operation.as_str(), "update" | "delete") {
            let Some(rel) = f.before_object.as_deref() else {
                return Err(PublicError::new(
                    ErrorCode::ReceiptInvalid,
                    format!("Hashes-only receipt rejected for {}", f.path),
                ));
            };
            let Some(hex) = rel.strip_prefix("objects/") else {
                return Err(PublicError::new(
                    ErrorCode::ReceiptInvalid,
                    format!("Invalid before_object for {}", f.path),
                ));
            };
            if !object_path(root, hex).is_file() {
                return Err(PublicError::new(
                    ErrorCode::ReceiptObjectMissing,
                    format!("Missing object {hex} for {}", f.path),
                ));
            }
        }
    }
    Ok(())
}

pub fn list_receipt_paths(root: &Path) -> Result<Vec<PathBuf>, PublicError> {
    let dir = receipts_dir(root);
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for ent in fs::read_dir(&dir)
        .map_err(|e| PublicError::new(ErrorCode::IoError, format!("Cannot read receipts: {e}")))?
    {
        let ent = ent.map_err(|e| {
            PublicError::new(
                ErrorCode::IoError,
                format!("Cannot read receipt entry: {e}"),
            )
        })?;
        if ent.file_type().map(|t| t.is_file()).unwrap_or(false)
            && ent.path().extension().and_then(|e| e.to_str()) == Some("json")
        {
            out.push(ent.path());
        }
    }
    out.sort();
    Ok(out)
}

fn write_atomic(path: &Path, receipt: &ApplyReceipt) -> Result<(), PublicError> {
    let bytes = serde_json::to_vec_pretty(receipt).map_err(|e| {
        PublicError::new(ErrorCode::InternalError, format!("Receipt serialize: {e}"))
    })?;
    let tmp = path.with_extension("json.tmp");
    {
        let mut f = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp)
            .map_err(|e| {
                PublicError::new(
                    ErrorCode::IoError,
                    format!("Cannot write receipt temp: {e}"),
                )
            })?;
        f.write_all(&bytes).map_err(|e| {
            PublicError::new(ErrorCode::IoError, format!("Cannot write receipt: {e}"))
        })?;
        f.sync_all().map_err(|e| {
            PublicError::new(ErrorCode::IoError, format!("Cannot fsync receipt: {e}"))
        })?;
    }
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        PublicError::new(
            ErrorCode::IoError,
            format!("Cannot publish receipt {}: {e}", path.display()),
        )
    })?;
    if let Some(parent) = path.parent() {
        if let Ok(d) = File::open(parent) {
            let _ = d.sync_all();
        }
    }
    Ok(())
}

fn now_secs() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::{EntryProgress, JournalEntry};
    use crate::objects::{object_rel_path, put_object};

    #[test]
    fn rejects_hashes_only_update() {
        let dir = tempfile::tempdir().unwrap();
        let receipt = ApplyReceipt {
            version: 2,
            root: dir.path().display().to_string(),
            created_at: "0".into(),
            transaction_id: "t1".into(),
            plan_digest: "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .into(),
            tool_contract_version: 2,
            files: vec![ReceiptFile {
                path: "a.txt".into(),
                operation: "update".into(),
                before_blake3: Some("bb".into()),
                after_blake3: Some("cc".into()),
                before_object: None,
                permissions: None,
            }],
        };
        let err = validate_objects(dir.path(), &receipt).unwrap_err();
        assert_eq!(err.code, ErrorCode::ReceiptInvalid);
    }

    #[test]
    fn write_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let hex = put_object(dir.path(), b"before\n").unwrap();
        let receipt = ApplyReceipt {
            version: 2,
            root: dir.path().display().to_string(),
            created_at: "0".into(),
            transaction_id: "t2".into(),
            plan_digest: "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .into(),
            tool_contract_version: 2,
            files: vec![ReceiptFile {
                path: "a.txt".into(),
                operation: "update".into(),
                before_blake3: Some(hex.clone()),
                after_blake3: Some("cc".into()),
                before_object: Some(object_rel_path(&hex)),
                permissions: Some(ReceiptPermissions {
                    executable: false,
                    mode: Some(0o644),
                }),
            }],
        };
        write_internal(dir.path(), &receipt).unwrap();
        let loaded = load(&receipts_dir(dir.path()).join("t2.json")).unwrap();
        assert_eq!(loaded.transaction_id, "t2");
        let _ = JournalEntry {
            path: "a.txt".into(),
            operation: "update".into(),
            before_object: Some(object_rel_path(&hex)),
            after_blake3: Some("cc".into()),
            temp_path: None,
            progress: EntryProgress::Done,
        };
    }
}
