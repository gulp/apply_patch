//! Filesystem snapshot loader.

use crate::engine::{detect_newline_style, has_final_newline};
use crate::error::{ContentFingerprint, ErrorCode, Limits, NewlineStyle, PublicError};
use crate::fs::{FileSystem, FsMetadata};
use crate::path_policy::{resolve_under_root, CanonicalRoot, RepoPath};
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct FileSnapshot {
    pub path: RepoPath,
    pub abs_path: std::path::PathBuf,
    pub state: FileState,
}

#[derive(Debug, Clone)]
pub enum FileState {
    Missing,
    Present(PresentFile),
}

#[derive(Debug, Clone)]
pub struct PresentFile {
    pub bytes: Vec<u8>,
    pub text: String,
    pub fingerprint: ContentFingerprint,
    pub permissions: u32,
    pub newline_style: NewlineStyle,
    pub final_newline: bool,
    pub size: u64,
    pub modified: Option<std::time::SystemTime>,
}

pub fn load_snapshots(
    fs: &dyn FileSystem,
    root: &CanonicalRoot,
    paths: &[RepoPath],
    limits: &Limits,
) -> Result<BTreeMap<RepoPath, FileSnapshot>, PublicError> {
    let mut map = BTreeMap::new();
    for path in paths {
        let abs = resolve_under_root(root, path)?;
        let snap = load_one(fs, path.clone(), abs, limits)?;
        map.insert(path.clone(), snap);
    }
    Ok(map)
}

fn load_one(
    fs: &dyn FileSystem,
    path: RepoPath,
    abs: std::path::PathBuf,
    limits: &Limits,
) -> Result<FileSnapshot, PublicError> {
    let meta = match fs.symlink_metadata(&abs) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(FileSnapshot {
                path,
                abs_path: abs,
                state: FileState::Missing,
            });
        }
        Err(e) => {
            return Err(
                PublicError::new(ErrorCode::IoError, format!("Cannot stat {}: {e}", path))
                    .with_path(path.as_str()),
            );
        }
    };

    if meta.is_symlink {
        return Err(PublicError::new(
            ErrorCode::NotRegularFile,
            format!("Path {} is a symbolic link; refusing to patch.", path),
        )
        .with_path(path.as_str()));
    }
    if meta.is_dir {
        return Err(PublicError::new(
            ErrorCode::NotRegularFile,
            format!("Path {} is a directory; refusing to patch.", path),
        )
        .with_path(path.as_str()));
    }
    if !meta.is_file {
        return Err(PublicError::new(
            ErrorCode::NotRegularFile,
            format!("Path {} is not a regular file.", path),
        )
        .with_path(path.as_str()));
    }

    if meta.len > limits.max_file_bytes as u64 {
        return Err(PublicError::new(
            ErrorCode::LimitFileBytes,
            format!(
                "File {} exceeds max-file-bytes limit ({} > {}).",
                path, meta.len, limits.max_file_bytes
            ),
        )
        .with_path(path.as_str()));
    }

    let bytes = fs.read(&abs).map_err(|e| {
        PublicError::new(ErrorCode::IoError, format!("Cannot read {}: {e}", path))
            .with_path(path.as_str())
    })?;

    if bytes.contains(&0) {
        return Err(PublicError::new(
            ErrorCode::BinaryFileUnsupported,
            format!("File {} appears binary (contains NUL).", path),
        )
        .with_path(path.as_str()));
    }

    let text = match String::from_utf8(bytes.clone()) {
        Ok(t) => t,
        Err(_) => {
            return Err(PublicError::new(
                ErrorCode::InvalidUtf8,
                format!("File {} is not valid UTF-8.", path),
            )
            .with_path(path.as_str()));
        }
    };

    let newline_style = detect_newline_style(&text);
    let final_newline = has_final_newline(&text);
    let fingerprint = ContentFingerprint::blake3(&bytes);

    Ok(FileSnapshot {
        path,
        abs_path: abs,
        state: FileState::Present(PresentFile {
            bytes,
            text,
            fingerprint,
            permissions: meta.mode,
            newline_style,
            final_newline,
            size: meta.len,
            modified: meta.modified,
        }),
    })
}

/// Recompute identity for concurrency check.
pub fn current_identity(
    fs: &dyn FileSystem,
    abs: &std::path::Path,
    limits: &Limits,
) -> Result<(bool, Option<ContentFingerprint>), PublicError> {
    match fs.symlink_metadata(abs) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok((false, None)),
        Err(e) => Err(PublicError::new(
            ErrorCode::IoError,
            format!("Cannot stat {}: {e}", abs.display()),
        )),
        Ok(meta) => {
            if !meta.is_file || meta.is_symlink {
                return Err(PublicError::new(
                    ErrorCode::ConcurrentModification,
                    format!("Path {} changed type before commit.", abs.display()),
                ));
            }
            if meta.len > limits.max_file_bytes as u64 {
                return Err(PublicError::new(
                    ErrorCode::LimitFileBytes,
                    "File grew beyond limit before commit.",
                ));
            }
            let bytes = fs
                .read(abs)
                .map_err(|e| PublicError::new(ErrorCode::IoError, format!("Cannot read: {e}")))?;
            Ok((true, Some(ContentFingerprint::blake3(&bytes))))
        }
    }
}

#[allow(dead_code)]
fn _meta_mode(m: &FsMetadata) -> u32 {
    m.mode
}
