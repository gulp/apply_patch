//! `.agent-patch/` layout helpers.

use crate::error::{ErrorCode, PublicError};
use std::path::{Path, PathBuf};

pub const DIR_NAME: &str = ".agent-patch";
pub const LOCK_NAME: &str = "lock";
pub const OBJECTS_DIR: &str = "objects";
pub const TRANSACTIONS_DIR: &str = "transactions";
pub const RECEIPTS_DIR: &str = "receipts";
pub const EVENTS_DIR: &str = "events";
pub const SHADOWS_DIR: &str = "shadows";

pub fn agent_patch_dir(root: &Path) -> PathBuf {
    root.join(DIR_NAME)
}

pub fn ensure_layout(root: &Path) -> Result<PathBuf, PublicError> {
    let base = agent_patch_dir(root);
    for sub in [OBJECTS_DIR, TRANSACTIONS_DIR, RECEIPTS_DIR, EVENTS_DIR, SHADOWS_DIR] {
        let p = base.join(sub);
        std::fs::create_dir_all(&p).map_err(|e| {
            PublicError::new(
                ErrorCode::IoError,
                format!("Cannot create {}: {e}", p.display()),
            )
        })?;
    }
    Ok(base)
}

pub fn objects_dir(root: &Path) -> PathBuf {
    agent_patch_dir(root).join(OBJECTS_DIR)
}

pub fn transactions_dir(root: &Path) -> PathBuf {
    agent_patch_dir(root).join(TRANSACTIONS_DIR)
}

pub fn receipts_dir(root: &Path) -> PathBuf {
    agent_patch_dir(root).join(RECEIPTS_DIR)
}

pub fn lock_path(root: &Path) -> PathBuf {
    agent_patch_dir(root).join(LOCK_NAME)
}
