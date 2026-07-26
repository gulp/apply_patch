//! Shared types, error codes, and limits for agent-patch.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Stable public error codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    PatchMissingBegin,
    PatchMissingEnd,
    PatchEmpty,
    PatchTrailingContent,
    UnknownOperation,
    MalformedHunk,
    InvalidPath,
    PathOutsideRoot,
    SymlinkEscape,
    PathCollision,
    FileAlreadyExists,
    FileNotFound,
    NotRegularFile,
    BinaryFileUnsupported,
    InvalidUtf8,
    MixedLineEndings,
    HunkNotFound,
    HunkAmbiguous,
    HunkOverlap,
    PatchNoEffect,
    ConcurrentModification,
    AtomicCommitFailed,
    RollbackFailed,
    LimitPatchBytes,
    LimitFileBytes,
    LimitFileCount,
    LimitHunkCount,
    IoError,
    InternalError,
    InputError,
    DuplicatePath,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PatchMissingBegin => "PATCH_MISSING_BEGIN",
            Self::PatchMissingEnd => "PATCH_MISSING_END",
            Self::PatchEmpty => "PATCH_EMPTY",
            Self::PatchTrailingContent => "PATCH_TRAILING_CONTENT",
            Self::UnknownOperation => "UNKNOWN_OPERATION",
            Self::MalformedHunk => "MALFORMED_HUNK",
            Self::InvalidPath => "INVALID_PATH",
            Self::PathOutsideRoot => "PATH_OUTSIDE_ROOT",
            Self::SymlinkEscape => "SYMLINK_ESCAPE",
            Self::PathCollision => "PATH_COLLISION",
            Self::FileAlreadyExists => "FILE_ALREADY_EXISTS",
            Self::FileNotFound => "FILE_NOT_FOUND",
            Self::NotRegularFile => "NOT_REGULAR_FILE",
            Self::BinaryFileUnsupported => "BINARY_FILE_UNSUPPORTED",
            Self::InvalidUtf8 => "INVALID_UTF8",
            Self::MixedLineEndings => "MIXED_LINE_ENDINGS",
            Self::HunkNotFound => "HUNK_NOT_FOUND",
            Self::HunkAmbiguous => "HUNK_AMBIGUOUS",
            Self::HunkOverlap => "HUNK_OVERLAP",
            Self::PatchNoEffect => "PATCH_NO_EFFECT",
            Self::ConcurrentModification => "CONCURRENT_MODIFICATION",
            Self::AtomicCommitFailed => "ATOMIC_COMMIT_FAILED",
            Self::RollbackFailed => "ROLLBACK_FAILED",
            Self::LimitPatchBytes => "LIMIT_PATCH_BYTES",
            Self::LimitFileBytes => "LIMIT_FILE_BYTES",
            Self::LimitFileCount => "LIMIT_FILE_COUNT",
            Self::LimitHunkCount => "LIMIT_HUNK_COUNT",
            Self::IoError => "IO_ERROR",
            Self::InternalError => "INTERNAL_ERROR",
            Self::InputError => "INPUT_ERROR",
            Self::DuplicatePath => "DUPLICATE_PATH",
        }
    }

    pub fn exit_code(self) -> u8 {
        match self {
            Self::HunkNotFound
            | Self::HunkAmbiguous
            | Self::HunkOverlap
            | Self::PatchNoEffect
            | Self::FileAlreadyExists
            | Self::FileNotFound => 1,
            Self::PatchMissingBegin
            | Self::PatchMissingEnd
            | Self::PatchEmpty
            | Self::PatchTrailingContent
            | Self::UnknownOperation
            | Self::MalformedHunk
            | Self::BinaryFileUnsupported
            | Self::InvalidUtf8
            | Self::MixedLineEndings
            | Self::InputError
            | Self::DuplicatePath => 2,
            Self::IoError | Self::AtomicCommitFailed => 3,
            Self::InvalidPath
            | Self::PathOutsideRoot
            | Self::SymlinkEscape
            | Self::PathCollision
            | Self::NotRegularFile => 4,
            Self::ConcurrentModification => 5,
            Self::RollbackFailed | Self::InternalError => 6,
            Self::LimitPatchBytes
            | Self::LimitFileBytes
            | Self::LimitFileCount
            | Self::LimitHunkCount => 7,
        }
    }

    pub fn default_hint(self) -> &'static str {
        match self {
            Self::HunkNotFound | Self::HunkAmbiguous => {
                "Read the current affected region and regenerate the patch from current content."
            }
            Self::ConcurrentModification => {
                "Reread affected files and regenerate the patch; do not retry blindly."
            }
            Self::InvalidPath | Self::PathOutsideRoot | Self::SymlinkEscape => {
                "Use a repository-relative path that stays inside the configured root."
            }
            Self::FileAlreadyExists => "Use Update File for existing paths, or delete first.",
            Self::FileNotFound => "Use Add File for missing paths, or fix the path.",
            Self::PatchNoEffect => "Ensure the patch changes at least one byte.",
            Self::LimitPatchBytes
            | Self::LimitFileBytes
            | Self::LimitFileCount
            | Self::LimitHunkCount => "Reduce patch size or raise the corresponding --max-* limit.",
            _ => "Fix the reported issue and retry.",
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceSpan {
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone)]
pub struct PublicError {
    pub code: ErrorCode,
    pub message: String,
    pub path: Option<String>,
    pub operation_index: Option<usize>,
    pub hunk_index: Option<usize>,
    pub source: Option<SourceSpan>,
    pub hint: Option<String>,
}

impl PublicError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            path: None,
            operation_index: None,
            hunk_index: None,
            source: None,
            hint: Some(code.default_hint().to_string()),
        }
    }

    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    pub fn with_operation(mut self, index: usize) -> Self {
        self.operation_index = Some(index);
        self
    }

    pub fn with_hunk(mut self, index: usize) -> Self {
        self.hunk_index = Some(index);
        self
    }

    pub fn with_source(mut self, span: SourceSpan) -> Self {
        self.source = Some(span);
        self
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn exit_code(&self) -> u8 {
        self.code.exit_code()
    }
}

#[derive(Debug, Clone)]
pub struct Limits {
    pub max_patch_bytes: usize,
    pub max_file_bytes: usize,
    pub max_files: usize,
    pub max_hunks_per_file: usize,
    pub max_total_hunks: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_patch_bytes: 4 * 1024 * 1024,
            max_file_bytes: 16 * 1024 * 1024,
            max_files: 128,
            max_hunks_per_file: 256,
            max_total_hunks: 2048,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewlineStyle {
    Lf,
    CrLf,
    Mixed,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentFingerprint {
    pub algorithm: &'static str,
    pub hash: [u8; 32],
}

impl ContentFingerprint {
    pub fn blake3(bytes: &[u8]) -> Self {
        let hash = *blake3::hash(bytes).as_bytes();
        Self {
            algorithm: "blake3",
            hash,
        }
    }

    pub fn hex(&self) -> String {
        self.hash.iter().map(|b| format!("{b:02x}")).collect()
    }
}
