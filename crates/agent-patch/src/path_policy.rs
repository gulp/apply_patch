//! Repository-relative path policy and root confinement.

use crate::error::{ErrorCode, PublicError};
use camino::{Utf8Path, Utf8PathBuf};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

/// Validated repository-relative path.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RepoPath {
    inner: String,
}

impl RepoPath {
    pub fn as_str(&self) -> &str {
        &self.inner
    }

    pub fn as_utf8_path(&self) -> &Utf8Path {
        Utf8Path::new(&self.inner)
    }
}

impl std::fmt::Display for RepoPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.inner)
    }
}

#[derive(Debug, Clone)]
pub struct CanonicalRoot {
    pub path: PathBuf,
}

impl CanonicalRoot {
    pub fn resolve(root: &Path) -> Result<Self, PublicError> {
        let path = fs::canonicalize(root).map_err(|e| {
            PublicError::new(
                ErrorCode::IoError,
                format!("Cannot canonicalize root {}: {e}", root.display()),
            )
        })?;
        Ok(Self { path })
    }

    pub fn join(&self, repo_path: &RepoPath) -> PathBuf {
        self.path.join(repo_path.as_str())
    }
}

/// Parse and validate a repository-relative path string.
pub fn parse_repo_path(raw: &str) -> Result<RepoPath, PublicError> {
    if raw.is_empty() {
        return Err(PublicError::new(
            ErrorCode::InvalidPath,
            "Path must not be empty.",
        ));
    }
    if raw.contains('\0') {
        return Err(PublicError::new(
            ErrorCode::InvalidPath,
            "Path must not contain NUL bytes.",
        ));
    }
    if raw.starts_with('/') || raw.starts_with('\\') {
        return Err(
            PublicError::new(ErrorCode::InvalidPath, "Absolute paths are forbidden.")
                .with_path(raw),
        );
    }
    // Windows drive-like
    if raw.len() >= 2 {
        let bytes = raw.as_bytes();
        if bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
            return Err(PublicError::new(
                ErrorCode::InvalidPath,
                "Windows-style absolute paths are forbidden.",
            )
            .with_path(raw));
        }
    }

    let path = Path::new(raw);
    let mut normalized = Utf8PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::Normal(s) => {
                let s = s.to_str().ok_or_else(|| {
                    PublicError::new(ErrorCode::InvalidPath, "Path is not valid UTF-8.")
                        .with_path(raw)
                })?;
                if s == ".." || s == "." {
                    return Err(PublicError::new(
                        ErrorCode::InvalidPath,
                        "Path must not contain '.' or '..' components.",
                    )
                    .with_path(raw));
                }
                normalized.push(s);
            }
            Component::CurDir => {
                return Err(PublicError::new(
                    ErrorCode::InvalidPath,
                    "Path must not contain '.' components.",
                )
                .with_path(raw));
            }
            Component::ParentDir => {
                return Err(PublicError::new(
                    ErrorCode::InvalidPath,
                    "Path must not contain '..' components.",
                )
                .with_path(raw));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(PublicError::new(
                    ErrorCode::InvalidPath,
                    "Absolute paths are forbidden.",
                )
                .with_path(raw));
            }
        }
    }

    if normalized.as_str().is_empty() {
        return Err(
            PublicError::new(ErrorCode::InvalidPath, "Path must not be empty.").with_path(raw),
        );
    }

    Ok(RepoPath {
        inner: normalized.into_string(),
    })
}

/// Securely resolve a repo path under root, rejecting symlink escapes.
pub fn resolve_under_root(
    root: &CanonicalRoot,
    repo_path: &RepoPath,
) -> Result<PathBuf, PublicError> {
    let mut current = root.path.clone();
    for comp in repo_path.as_utf8_path().components() {
        current.push(comp.as_str());
        // If this component exists, inspect symlink metadata without following
        match fs::symlink_metadata(&current) {
            Ok(meta) => {
                if meta.file_type().is_symlink() {
                    let target = fs::canonicalize(&current).map_err(|_| {
                        PublicError::new(
                            ErrorCode::SymlinkEscape,
                            format!("Cannot resolve symlink at {} under root.", repo_path),
                        )
                        .with_path(repo_path.as_str())
                    })?;
                    if !target.starts_with(&root.path) {
                        return Err(PublicError::new(
                            ErrorCode::SymlinkEscape,
                            format!("Symlink at {} escapes the configured root.", repo_path),
                        )
                        .with_path(repo_path.as_str()));
                    }
                    // Continue from canonical target for remaining components
                    current = target;
                } else if meta.is_dir() {
                    // keep walking
                } else {
                    // File mid-path — remaining components would be invalid; allow final
                    // component to be a file when it's the last one. Handled by caller.
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Remaining components are to-be-created; ensure no `..` already checked.
                // Continue pushing logical components without FS checks.
            }
            Err(e) => {
                return Err(PublicError::new(
                    ErrorCode::IoError,
                    format!("Cannot stat {}: {e}", current.display()),
                )
                .with_path(repo_path.as_str()));
            }
        }
    }

    // Final confinement check when path exists
    if current.exists() {
        let canon = fs::canonicalize(&current).map_err(|e| {
            PublicError::new(
                ErrorCode::IoError,
                format!("Cannot canonicalize {}: {e}", current.display()),
            )
            .with_path(repo_path.as_str())
        })?;
        if !canon.starts_with(&root.path) {
            return Err(PublicError::new(
                ErrorCode::PathOutsideRoot,
                format!("Path {} resolves outside the configured root.", repo_path),
            )
            .with_path(repo_path.as_str()));
        }
        return Ok(canon);
    }

    // For non-existent paths, ensure parent chain stays under root
    if let Some(parent) = current.parent() {
        if parent.exists() {
            let parent_canon = fs::canonicalize(parent).map_err(|e| {
                PublicError::new(
                    ErrorCode::IoError,
                    format!("Cannot canonicalize parent {}: {e}", parent.display()),
                )
                .with_path(repo_path.as_str())
            })?;
            if !parent_canon.starts_with(&root.path) {
                return Err(PublicError::new(
                    ErrorCode::PathOutsideRoot,
                    format!("Parent of {} is outside the configured root.", repo_path),
                )
                .with_path(repo_path.as_str()));
            }
            let name = current.file_name().ok_or_else(|| {
                PublicError::new(ErrorCode::InvalidPath, "Invalid path.")
                    .with_path(repo_path.as_str())
            })?;
            return Ok(parent_canon.join(name));
        }
    }

    // Parent does not exist yet — logical join under root is fine after parse_repo_path
    let logical = root.join(repo_path);
    if !logical.starts_with(&root.path) {
        return Err(PublicError::new(
            ErrorCode::PathOutsideRoot,
            format!("Path {} escapes the configured root.", repo_path),
        )
        .with_path(repo_path.as_str()));
    }
    Ok(logical)
}

/// Detect path alias collisions among a set of repo paths (lexicographic).
pub fn check_path_collisions(root: &CanonicalRoot, paths: &[RepoPath]) -> Result<(), PublicError> {
    let mut seen_abs: BTreeSet<PathBuf> = BTreeSet::new();
    for path in paths {
        let abs = resolve_under_root(root, path)?;
        // Only check collisions for existing paths
        if abs.exists() {
            let canon = fs::canonicalize(&abs).unwrap_or(abs.clone());
            if !seen_abs.insert(canon.clone()) {
                return Err(PublicError::new(
                    ErrorCode::PathCollision,
                    format!(
                        "Path {} aliases another operation target on the filesystem.",
                        path
                    ),
                )
                .with_path(path.as_str()));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_dotdot() {
        assert_eq!(
            parse_repo_path("a/../b").unwrap_err().code,
            ErrorCode::InvalidPath
        );
    }

    #[test]
    fn rejects_absolute() {
        assert_eq!(
            parse_repo_path("/etc/passwd").unwrap_err().code,
            ErrorCode::InvalidPath
        );
    }

    #[test]
    fn accepts_normal() {
        let p = parse_repo_path("src/main.rs").unwrap();
        assert_eq!(p.as_str(), "src/main.rs");
    }

    #[test]
    fn normalizes_repeated_separators() {
        let p = parse_repo_path("src//main.rs").unwrap();
        assert_eq!(p.as_str(), "src/main.rs");
    }
}
