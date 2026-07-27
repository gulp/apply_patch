//! Durable transaction journal (contract-v2 states).

use crate::error::{ErrorCode, PublicError};
use crate::store_layout::{ensure_layout, transactions_dir};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum JournalState {
    Prepared,
    Committing,
    Completed,
    RollingBack,
    RolledBack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryProgress {
    Pending,
    Visible,
    Done,
    RolledBack,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntry {
    pub path: String,
    pub operation: String,
    pub before_object: Option<String>,
    pub after_blake3: Option<String>,
    pub temp_path: Option<String>,
    pub progress: EntryProgress,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionJournal {
    pub version: u32,
    pub transaction_id: String,
    pub plan_digest: String,
    pub state: JournalState,
    pub created_at: String,
    pub entries: Vec<JournalEntry>,
}

impl TransactionJournal {
    pub fn new(transaction_id: String, plan_digest: String, entries: Vec<JournalEntry>) -> Self {
        Self {
            version: 2,
            transaction_id,
            plan_digest,
            state: JournalState::Prepared,
            created_at: now_rfc3339ish(),
            entries,
        }
    }

    pub fn dir(root: &Path, txid: &str) -> PathBuf {
        transactions_dir(root).join(txid)
    }

    pub fn path(root: &Path, txid: &str) -> PathBuf {
        Self::dir(root, txid).join("journal.json")
    }

    pub fn write_durable(&self, root: &Path) -> Result<(), PublicError> {
        ensure_layout(root)?;
        let dir = Self::dir(root, &self.transaction_id);
        fs::create_dir_all(&dir).map_err(|e| {
            PublicError::new(
                ErrorCode::IoError,
                format!("Cannot create transaction dir: {e}"),
            )
        })?;
        let path = Self::path(root, &self.transaction_id);
        let tmp = dir.join("journal.json.tmp");
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| {
            PublicError::new(ErrorCode::InternalError, format!("Journal serialize: {e}"))
        })?;
        {
            let mut f = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&tmp)
                .map_err(|e| {
                    PublicError::new(
                        ErrorCode::IoError,
                        format!("Cannot write journal temp: {e}"),
                    )
                })?;
            f.write_all(&bytes).map_err(|e| {
                PublicError::new(ErrorCode::IoError, format!("Cannot write journal: {e}"))
            })?;
            f.sync_all().map_err(|e| {
                PublicError::new(ErrorCode::IoError, format!("Cannot fsync journal: {e}"))
            })?;
        }
        fs::rename(&tmp, &path).map_err(|e| {
            PublicError::new(ErrorCode::IoError, format!("Cannot publish journal: {e}"))
        })?;
        if let Ok(d) = File::open(&dir) {
            let _ = d.sync_all();
        }
        Ok(())
    }

    pub fn load(root: &Path, txid: &str) -> Result<Self, PublicError> {
        let path = Self::path(root, txid);
        let bytes = fs::read(&path).map_err(|e| {
            PublicError::new(
                ErrorCode::IoError,
                format!("Cannot read journal {}: {e}", path.display()),
            )
        })?;
        let j: Self = serde_json::from_slice(&bytes).map_err(|e| {
            PublicError::new(
                ErrorCode::InternalError,
                format!("Corrupt journal {}: {e}", path.display()),
            )
        })?;
        if j.version != 2 {
            return Err(PublicError::new(
                ErrorCode::InternalError,
                format!("Unsupported journal version {}", j.version),
            ));
        }
        Ok(j)
    }

    pub fn set_state(&mut self, state: JournalState) {
        self.state = state;
    }

    pub fn is_incomplete(&self) -> bool {
        !matches!(
            self.state,
            JournalState::Completed | JournalState::RolledBack
        )
    }
}

/// List transaction ids that still require recovery.
pub fn list_incomplete(root: &Path) -> Result<Vec<String>, PublicError> {
    let dir = transactions_dir(root);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for ent in fs::read_dir(&dir).map_err(|e| {
        PublicError::new(
            ErrorCode::IoError,
            format!("Cannot read transactions dir: {e}"),
        )
    })? {
        let ent = ent.map_err(|e| {
            PublicError::new(ErrorCode::IoError, format!("Cannot read dir entry: {e}"))
        })?;
        if !ent.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let txid = ent.file_name().to_string_lossy().into_owned();
        let journal_path = ent.path().join("journal.json");
        if !journal_path.exists() {
            continue;
        }
        let j = TransactionJournal::load(root, &txid)?;
        if j.is_incomplete() {
            out.push(txid);
        }
    }
    out.sort();
    Ok(out)
}

pub fn refuse_if_incomplete(root: &Path) -> Result<(), PublicError> {
    let incomplete = list_incomplete(root)?;
    if let Some(txid) = incomplete.first() {
        return Err(PublicError::new(
            ErrorCode::RecoveryRequired,
            format!(
                "Incomplete transaction {txid} exists; run `agent-patch recover` before mutating."
            ),
        )
        .with_hint(format!("agent-patch recover --transaction {txid}"))
        .with_recovery_required(true));
    }
    Ok(())
}

pub fn new_transaction_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:016x}-{:08x}", nanos, std::process::id())
}

fn now_rfc3339ish() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn write_load_roundtrip() {
        let dir = tempdir().unwrap();
        let mut j = TransactionJournal::new(
            "tx1".into(),
            "blake3:abc".into(),
            vec![JournalEntry {
                path: "a.txt".into(),
                operation: "update".into(),
                before_object: Some("objects/aa".into()),
                after_blake3: Some("bb".into()),
                temp_path: None,
                progress: EntryProgress::Pending,
            }],
        );
        j.write_durable(dir.path()).unwrap();
        let loaded = TransactionJournal::load(dir.path(), "tx1").unwrap();
        assert_eq!(loaded.state, JournalState::Prepared);
        assert!(loaded.is_incomplete());
        j.set_state(JournalState::Completed);
        j.write_durable(dir.path()).unwrap();
        assert!(list_incomplete(dir.path()).unwrap().is_empty());
    }

    #[test]
    fn refuse_when_incomplete() {
        let dir = tempdir().unwrap();
        let j = TransactionJournal::new("tx2".into(), "blake3:x".into(), vec![]);
        j.write_durable(dir.path()).unwrap();
        let err = refuse_if_incomplete(dir.path()).unwrap_err();
        assert_eq!(err.code, ErrorCode::RecoveryRequired);
    }
}
