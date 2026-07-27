//! Content-addressed before-image object store.

use crate::error::{ContentFingerprint, ErrorCode, PublicError};
use crate::store_layout::{ensure_layout, objects_dir};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Store `bytes` under `.agent-patch/objects/<blake3>` if absent. Returns the hex digest.
pub fn put_object(root: &Path, bytes: &[u8]) -> Result<String, PublicError> {
    ensure_layout(root)?;
    let fp = ContentFingerprint::blake3(bytes);
    let hex = fp.hex();
    let final_path = objects_dir(root).join(&hex);
    if final_path.exists() {
        verify_object(&final_path, &hex)?;
        return Ok(hex);
    }

    let tmp_path = objects_dir(root).join(format!(".{hex}.tmp"));
    {
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)
            .map_err(|e| {
                PublicError::new(
                    ErrorCode::IoError,
                    format!("Cannot create object temp {}: {e}", tmp_path.display()),
                )
            })?;
        f.write_all(bytes).map_err(|e| {
            PublicError::new(ErrorCode::IoError, format!("Cannot write object: {e}"))
        })?;
        f.sync_all().map_err(|e| {
            PublicError::new(ErrorCode::IoError, format!("Cannot fsync object: {e}"))
        })?;
    }
    // Verify before publish
    verify_object(&tmp_path, &hex)?;
    fs::rename(&tmp_path, &final_path).map_err(|e| {
        let _ = fs::remove_file(&tmp_path);
        PublicError::new(
            ErrorCode::IoError,
            format!("Cannot publish object {}: {e}", final_path.display()),
        )
    })?;
    // Best-effort directory fsync
    if let Ok(dir) = File::open(objects_dir(root)) {
        let _ = dir.sync_all();
    }
    Ok(hex)
}

pub fn get_object(root: &Path, hex: &str) -> Result<Vec<u8>, PublicError> {
    let path = object_path(root, hex);
    let bytes = fs::read(&path).map_err(|e| {
        PublicError::new(
            ErrorCode::ReceiptObjectMissing,
            format!("Missing object {}: {e}", path.display()),
        )
    })?;
    verify_bytes(&bytes, hex)?;
    Ok(bytes)
}

pub fn object_path(root: &Path, hex: &str) -> PathBuf {
    objects_dir(root).join(hex)
}

pub fn object_rel_path(hex: &str) -> String {
    format!("objects/{hex}")
}

fn verify_object(path: &Path, expected_hex: &str) -> Result<(), PublicError> {
    let bytes = fs::read(path).map_err(|e| {
        PublicError::new(
            ErrorCode::IoError,
            format!("Cannot reread object {}: {e}", path.display()),
        )
    })?;
    verify_bytes(&bytes, expected_hex)
}

fn verify_bytes(bytes: &[u8], expected_hex: &str) -> Result<(), PublicError> {
    let actual = ContentFingerprint::blake3(bytes).hex();
    if actual != expected_hex {
        return Err(PublicError::new(
            ErrorCode::InternalError,
            format!("Object hash mismatch: expected {expected_hex}, got {actual}"),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn put_get_roundtrip() {
        let dir = tempdir().unwrap();
        let hex = put_object(dir.path(), b"hello").unwrap();
        assert_eq!(get_object(dir.path(), &hex).unwrap(), b"hello");
        // idempotent
        let hex2 = put_object(dir.path(), b"hello").unwrap();
        assert_eq!(hex, hex2);
    }

    #[test]
    fn missing_object_fails() {
        let dir = tempdir().unwrap();
        ensure_layout(dir.path()).unwrap();
        let err = get_object(dir.path(), &"0".repeat(64)).unwrap_err();
        assert_eq!(err.code, ErrorCode::ReceiptObjectMissing);
    }
}
