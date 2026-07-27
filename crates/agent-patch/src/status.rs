//! Non-mutating health report for lock / journal / object store.

use crate::error::PublicError;
use crate::journal::list_incomplete;
use crate::store_layout::{agent_patch_dir, lock_path, objects_dir, transactions_dir};
use serde::Serialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct StatusReport {
    pub root: String,
    pub store_present: bool,
    pub lock_held: bool,
    pub lock_owner: Option<String>,
    pub incomplete_transactions: Vec<String>,
    pub object_count: usize,
    pub ok: bool,
    pub checks: Vec<StatusCheck>,
}

#[derive(Debug, Serialize)]
pub struct StatusCheck {
    pub name: String,
    pub level: String,
    pub message: String,
}

pub fn status(root: &Path) -> Result<StatusReport, PublicError> {
    let store = agent_patch_dir(root);
    let store_present = store.is_dir();
    let lock = lock_path(root);
    let lock_held = lock.exists();
    let lock_owner = if lock_held {
        fs::read_to_string(&lock).ok().map(|s| s.trim().to_string())
    } else {
        None
    };

    let incomplete = if store_present {
        list_incomplete(root)?
    } else {
        Vec::new()
    };

    let object_count = if objects_dir(root).is_dir() {
        fs::read_dir(objects_dir(root))
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter(|e| {
                        e.file_type().map(|t| t.is_file()).unwrap_or(false)
                            && !e.file_name().to_string_lossy().starts_with('.')
                    })
                    .count()
            })
            .unwrap_or(0)
    } else {
        0
    };

    let mut checks = Vec::new();
    if !store_present {
        checks.push(StatusCheck {
            name: "store".into(),
            level: "ok".into(),
            message: "No .agent-patch store yet (idle).".into(),
        });
    } else {
        checks.push(StatusCheck {
            name: "store".into(),
            level: "ok".into(),
            message: format!(
                "Store present at {}; {} objects; transactions dir {}.",
                store.display(),
                object_count,
                if transactions_dir(root).is_dir() {
                    "present"
                } else {
                    "missing"
                }
            ),
        });
    }

    if lock_held {
        checks.push(StatusCheck {
            name: "lock".into(),
            level: "warn".into(),
            message: format!(
                "Lock file present ({})",
                lock_owner.as_deref().unwrap_or("unknown owner")
            ),
        });
    } else {
        checks.push(StatusCheck {
            name: "lock".into(),
            level: "ok".into(),
            message: "No lock held.".into(),
        });
    }

    if incomplete.is_empty() {
        checks.push(StatusCheck {
            name: "journal".into(),
            level: "ok".into(),
            message: "No incomplete transactions.".into(),
        });
    } else {
        checks.push(StatusCheck {
            name: "journal".into(),
            level: "error".into(),
            message: format!(
                "{} incomplete transaction(s); run `agent-patch recover`.",
                incomplete.len()
            ),
        });
    }

    let ok = checks.iter().all(|c| c.level != "error");
    Ok(StatusReport {
        root: root.display().to_string(),
        store_present,
        lock_held,
        lock_owner,
        incomplete_transactions: incomplete,
        object_count,
        ok,
        checks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::TransactionJournal;

    #[test]
    fn status_flags_incomplete() {
        let dir = tempfile::tempdir().unwrap();
        let report = status(dir.path()).unwrap();
        assert!(report.ok);
        let j = TransactionJournal::new("tx".into(), "blake3:x".into(), vec![]);
        j.write_durable(dir.path()).unwrap();
        let report = status(dir.path()).unwrap();
        assert!(!report.ok);
        assert_eq!(report.incomplete_transactions, vec!["tx".to_string()]);
    }
}
