//! Protocol AST types.

use crate::error::SourceSpan;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchDocument {
    pub operations: Vec<FileOperation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileOperation {
    Add(AddFile),
    Update(UpdateFile),
    Delete(DeleteFile),
}

impl FileOperation {
    pub fn path(&self) -> &str {
        match self {
            Self::Add(op) => &op.path,
            Self::Update(op) => &op.path,
            Self::Delete(op) => &op.path,
        }
    }

    pub fn source_span(&self) -> SourceSpan {
        match self {
            Self::Add(op) => op.source_span,
            Self::Update(op) => op.source_span,
            Self::Delete(op) => op.source_span,
        }
    }

    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Add(_) => "add",
            Self::Update(_) => "update",
            Self::Delete(_) => "delete",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddFile {
    pub path: String,
    pub content: String,
    pub source_span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateFile {
    pub path: String,
    pub hunks: Vec<Hunk>,
    pub source_span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteFile {
    pub path: String,
    pub source_span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    pub lines: Vec<HunkLine>,
    pub source_span: SourceSpan,
    /// Exact line from `@@ <anchor>`; `None` for bare `@@` or unified-diff numeric headers.
    pub anchor: Option<String>,
    /// True when the hunk ends with `*** End of File` (EOF-prefer locate).
    pub end_of_file: bool,
}

impl Hunk {
    pub fn old_text_lines(&self) -> Vec<&str> {
        self.lines
            .iter()
            .filter_map(|l| match l {
                HunkLine::Context(s) | HunkLine::Delete(s) => Some(s.as_str()),
                HunkLine::Add(_) => None,
            })
            .collect()
    }

    pub fn new_text_lines(&self) -> Vec<&str> {
        self.lines
            .iter()
            .filter_map(|l| match l {
                HunkLine::Context(s) | HunkLine::Add(s) => Some(s.as_str()),
                HunkLine::Delete(_) => None,
            })
            .collect()
    }

    pub fn has_change(&self) -> bool {
        self.lines
            .iter()
            .any(|l| matches!(l, HunkLine::Add(_) | HunkLine::Delete(_)))
    }

    pub fn lines_added(&self) -> usize {
        self.lines
            .iter()
            .filter(|l| matches!(l, HunkLine::Add(_)))
            .count()
    }

    pub fn lines_deleted(&self) -> usize {
        self.lines
            .iter()
            .filter(|l| matches!(l, HunkLine::Delete(_)))
            .count()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HunkLine {
    Context(String),
    Add(String),
    Delete(String),
}
