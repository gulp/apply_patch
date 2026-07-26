//! Patch input reader with size limits.

use crate::error::{ErrorCode, PublicError};
use std::fs;
use std::io::{self, Read};
use std::path::Path;

pub fn read_patch_bytes(
    patch_file: Option<&Path>,
    max_bytes: usize,
) -> Result<Vec<u8>, PublicError> {
    match patch_file {
        Some(path) => {
            let meta = fs::metadata(path).map_err(|e| {
                PublicError::new(
                    ErrorCode::InputError,
                    format!("Cannot open patch file {}: {e}", path.display()),
                )
            })?;
            if meta.len() as usize > max_bytes {
                return Err(PublicError::new(
                    ErrorCode::LimitPatchBytes,
                    format!(
                        "Patch file exceeds max-patch-bytes ({} > {}).",
                        meta.len(),
                        max_bytes
                    ),
                ));
            }
            let bytes = fs::read(path).map_err(|e| {
                PublicError::new(
                    ErrorCode::InputError,
                    format!("Cannot read patch file {}: {e}", path.display()),
                )
            })?;
            if bytes.len() > max_bytes {
                return Err(PublicError::new(
                    ErrorCode::LimitPatchBytes,
                    format!(
                        "Patch exceeds max-patch-bytes ({} > {}).",
                        bytes.len(),
                        max_bytes
                    ),
                ));
            }
            Ok(bytes)
        }
        None => read_stdin_limited(max_bytes),
    }
}

fn read_stdin_limited(max_bytes: usize) -> Result<Vec<u8>, PublicError> {
    let mut stdin = io::stdin().lock();
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let n = stdin.read(&mut chunk).map_err(|e| {
            PublicError::new(ErrorCode::InputError, format!("Failed reading stdin: {e}"))
        })?;
        if n == 0 {
            break;
        }
        if buf.len() + n > max_bytes {
            return Err(PublicError::new(
                ErrorCode::LimitPatchBytes,
                format!("Patch exceeds max-patch-bytes ({max_bytes})."),
            ));
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    Ok(buf)
}
