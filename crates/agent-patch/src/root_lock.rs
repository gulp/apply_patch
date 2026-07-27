//! Exclusive root mutation lock (advisory; never deletes journals).

use crate::error::{ErrorCode, PublicError};
use crate::store_layout::{ensure_layout, lock_path};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct RootLock {
    path: PathBuf,
    _file: File,
}

impl RootLock {
    /// Acquire exclusive lock with a bounded wait.
    pub fn acquire(root: &Path, timeout: Duration) -> Result<Self, PublicError> {
        ensure_layout(root)?;
        let path = lock_path(root);
        let deadline = Instant::now() + timeout;
        loop {
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(mut f) => {
                    let meta = format!("pid={}\n", std::process::id());
                    let _ = f.write_all(meta.as_bytes());
                    let _ = f.sync_all();
                    return Ok(Self { path, _file: f });
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    if Instant::now() >= deadline {
                        let owner = fs::read_to_string(&path).unwrap_or_default();
                        return Err(PublicError::new(
                            ErrorCode::RootLocked,
                            format!(
                                "Root is locked by another agent-patch writer ({}).",
                                owner.trim()
                            ),
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(e) => {
                    return Err(PublicError::new(
                        ErrorCode::IoError,
                        format!("Cannot acquire lock {}: {e}", path.display()),
                    ));
                }
            }
        }
    }
}

impl Drop for RootLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn second_acquire_times_out() {
        let dir = tempdir().unwrap();
        let _a = RootLock::acquire(dir.path(), Duration::from_millis(50)).unwrap();
        let err = RootLock::acquire(dir.path(), Duration::from_millis(80)).unwrap_err();
        assert_eq!(err.code, ErrorCode::RootLocked);
    }

    #[test]
    fn release_allows_reacquire() {
        let dir = tempdir().unwrap();
        {
            let _a = RootLock::acquire(dir.path(), Duration::from_millis(50)).unwrap();
        }
        let _b = RootLock::acquire(dir.path(), Duration::from_millis(50)).unwrap();
    }
}
