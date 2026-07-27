//! Reference-safe object GC.

use crate::error::{ErrorCode, PublicError};
use crate::journal::{list_incomplete, TransactionJournal};
use crate::objects::object_path;
use crate::receipt;
use crate::root_lock::RootLock;
use crate::store_layout::{objects_dir, transactions_dir};
use serde::Serialize;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Serialize)]
pub struct GcResult {
    pub root: String,
    pub dry_run: bool,
    pub referenced: usize,
    pub unreferenced: Vec<String>,
    pub deleted: Vec<String>,
}

pub fn gc(root: &Path, dry_run: bool) -> Result<GcResult, PublicError> {
    let _lock = RootLock::acquire(root, Duration::from_secs(30))?;
    let refs = collect_refs(root)?;
    let mut unreferenced = Vec::new();
    let dir = objects_dir(root);
    if dir.is_dir() {
        for ent in fs::read_dir(&dir).map_err(|e| {
            PublicError::new(ErrorCode::IoError, format!("Cannot read objects: {e}"))
        })? {
            let ent = ent.map_err(|e| {
                PublicError::new(ErrorCode::IoError, format!("Cannot read object entry: {e}"))
            })?;
            if !ent.file_type().map(|t| t.is_file()).unwrap_or(false) {
                continue;
            }
            let name = ent.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            if !refs.contains(&name) {
                unreferenced.push(name);
            }
        }
    }
    unreferenced.sort();

    let mut deleted = Vec::new();
    if !dry_run {
        for hex in &unreferenced {
            let path = object_path(root, hex);
            fs::remove_file(&path).map_err(|e| {
                PublicError::new(
                    ErrorCode::IoError,
                    format!("Cannot delete object {hex}: {e}"),
                )
            })?;
            deleted.push(hex.clone());
        }
    }

    Ok(GcResult {
        root: root.display().to_string(),
        dry_run,
        referenced: refs.len(),
        unreferenced,
        deleted,
    })
}

fn collect_refs(root: &Path) -> Result<BTreeSet<String>, PublicError> {
    let mut refs = BTreeSet::new();

    for path in receipt::list_receipt_paths(root)? {
        let receipt = receipt::load(&path)?;
        for f in &receipt.files {
            if let Some(rel) = &f.before_object {
                if let Some(hex) = rel.strip_prefix("objects/") {
                    refs.insert(hex.to_string());
                }
            }
        }
    }

    // Incomplete + all journals under transactions/
    let tx_dir = transactions_dir(root);
    if tx_dir.is_dir() {
        for ent in fs::read_dir(&tx_dir).map_err(|e| {
            PublicError::new(ErrorCode::IoError, format!("Cannot read transactions: {e}"))
        })? {
            let ent = ent.map_err(|e| {
                PublicError::new(ErrorCode::IoError, format!("Cannot read tx entry: {e}"))
            })?;
            if !ent.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let txid = ent.file_name().to_string_lossy().into_owned();
            if !ent.path().join("journal.json").exists() {
                continue;
            }
            let j = TransactionJournal::load(root, &txid)?;
            for e in &j.entries {
                if let Some(rel) = &e.before_object {
                    if let Some(hex) = rel.strip_prefix("objects/") {
                        refs.insert(hex.to_string());
                    }
                }
            }
        }
    }

    // Ensure incomplete are included (already covered by scan)
    let _ = list_incomplete(root)?;
    Ok(refs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::put_object;

    #[test]
    fn dry_run_lists_orphan() {
        let dir = tempfile::tempdir().unwrap();
        let keep = put_object(dir.path(), b"keep\n").unwrap();
        let drop = put_object(dir.path(), b"drop\n").unwrap();
        // Receipt references keep only
        let receipt = crate::receipt::ApplyReceipt {
            version: 2,
            root: dir.path().display().to_string(),
            created_at: "0".into(),
            transaction_id: "t".into(),
            plan_digest: "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .into(),
            tool_contract_version: 2,
            files: vec![crate::receipt::ReceiptFile {
                path: "a.txt".into(),
                operation: "update".into(),
                before_blake3: Some(keep.clone()),
                after_blake3: Some("x".into()),
                before_object: Some(format!("objects/{keep}")),
                permissions: None,
            }],
        };
        crate::receipt::write_internal(dir.path(), &receipt).unwrap();
        let r = gc(dir.path(), true).unwrap();
        assert!(r.unreferenced.contains(&drop));
        assert!(!r.unreferenced.contains(&keep));
        assert!(object_path(dir.path(), &drop).is_file());
        let r2 = gc(dir.path(), false).unwrap();
        assert!(r2.deleted.contains(&drop));
        assert!(!object_path(dir.path(), &drop).is_file());
        assert!(object_path(dir.path(), &keep).is_file());
    }
}
