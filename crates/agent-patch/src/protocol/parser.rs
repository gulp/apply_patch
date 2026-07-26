//! Patch protocol parser.

use super::ast::{AddFile, DeleteFile, FileOperation, Hunk, HunkLine, PatchDocument, UpdateFile};
use super::lexer::{split_lines, RawLine};
use crate::error::{ErrorCode, PublicError, SourceSpan};

pub fn parse_patch(input: &str) -> Result<PatchDocument, PublicError> {
    let lines = split_lines(input);
    parse_lines(&lines)
}

fn parse_lines(lines: &[RawLine<'_>]) -> Result<PatchDocument, PublicError> {
    let mut i = 0usize;

    // Skip leading blank lines
    while i < lines.len() && lines[i].text.trim().is_empty() {
        i += 1;
    }
    if i >= lines.len() {
        return Err(PublicError::new(
            ErrorCode::PatchMissingBegin,
            "Patch is empty; expected *** Begin Patch.",
        )
        .with_source(SourceSpan { line: 1, column: 1 }));
    }

    if lines[i].text != "*** Begin Patch" {
        // Non-whitespace before begin
        if !lines[i].text.trim().is_empty() {
            return Err(PublicError::new(
                ErrorCode::PatchTrailingContent,
                format!(
                    "Expected *** Begin Patch, found content at line {}: {:?}",
                    lines[i].line_no, lines[i].text
                ),
            )
            .with_source(SourceSpan {
                line: lines[i].line_no,
                column: 1,
            }));
        }
    }
    if lines[i].text != "*** Begin Patch" {
        return Err(PublicError::new(
            ErrorCode::PatchMissingBegin,
            "Missing *** Begin Patch marker.",
        )
        .with_source(SourceSpan {
            line: lines[i].line_no,
            column: 1,
        }));
    }
    i += 1;

    let mut operations = Vec::new();
    let mut saw_end = false;

    while i < lines.len() {
        let line = &lines[i];
        if line.text == "*** End Patch" {
            saw_end = true;
            i += 1;
            break;
        }
        if line.text.trim().is_empty() {
            i += 1;
            continue;
        }
        if line.text == "*** Begin Patch" {
            return Err(PublicError::new(
                ErrorCode::UnknownOperation,
                "Nested *** Begin Patch is not allowed.",
            )
            .with_source(SourceSpan {
                line: line.line_no,
                column: 1,
            }));
        }

        if let Some(path) = line.text.strip_prefix("*** Add File: ") {
            let span = SourceSpan {
                line: line.line_no,
                column: 1,
            };
            i += 1;
            let (content, next) = parse_add_body(lines, i)?;
            i = next;
            operations.push(FileOperation::Add(AddFile {
                path: path.to_string(),
                content,
                source_span: span,
            }));
        } else if let Some(path) = line.text.strip_prefix("*** Update File: ") {
            let span = SourceSpan {
                line: line.line_no,
                column: 1,
            };
            i += 1;
            let (hunks, next) = parse_update_body(lines, i)?;
            i = next;
            operations.push(FileOperation::Update(UpdateFile {
                path: path.to_string(),
                hunks,
                source_span: span,
            }));
        } else if let Some(path) = line.text.strip_prefix("*** Delete File: ") {
            let span = SourceSpan {
                line: line.line_no,
                column: 1,
            };
            i += 1;
            operations.push(FileOperation::Delete(DeleteFile {
                path: path.to_string(),
                source_span: span,
            }));
        } else if line.text.starts_with("*** Move File:") || line.text.starts_with("*** To:") {
            return Err(PublicError::new(
                ErrorCode::UnknownOperation,
                "Move File is not supported in v1; use Add/Delete or defer rename.",
            )
            .with_source(SourceSpan {
                line: line.line_no,
                column: 1,
            }));
        } else if line.text.starts_with("*** ") {
            return Err(PublicError::new(
                ErrorCode::UnknownOperation,
                format!("Unknown operation header: {}", line.text),
            )
            .with_source(SourceSpan {
                line: line.line_no,
                column: 1,
            }));
        } else {
            return Err(PublicError::new(
                ErrorCode::UnknownOperation,
                format!("Expected file operation header, found: {:?}", line.text),
            )
            .with_source(SourceSpan {
                line: line.line_no,
                column: 1,
            }));
        }
    }

    if !saw_end {
        return Err(PublicError::new(
            ErrorCode::PatchMissingEnd,
            "Missing *** End Patch marker.",
        ));
    }

    // Trailing content after end
    while i < lines.len() {
        if !lines[i].text.trim().is_empty() {
            return Err(PublicError::new(
                ErrorCode::PatchTrailingContent,
                format!(
                    "Non-whitespace content after *** End Patch at line {}",
                    lines[i].line_no
                ),
            )
            .with_source(SourceSpan {
                line: lines[i].line_no,
                column: 1,
            }));
        }
        i += 1;
    }

    if operations.is_empty() {
        return Err(PublicError::new(
            ErrorCode::PatchEmpty,
            "Patch contains no file operations.",
        ));
    }

    Ok(PatchDocument { operations })
}

fn parse_add_body(lines: &[RawLine<'_>], mut i: usize) -> Result<(String, usize), PublicError> {
    let mut content_lines = Vec::new();
    while i < lines.len() {
        let text = lines[i].text;
        if text.starts_with("*** ") {
            break;
        }
        if text.trim().is_empty() && !text.starts_with('+') {
            // Blank line between ops — stop if next is header, else require +
            // Treat blank as end of add body only when looking ahead isn't needed:
            // empty lines without '+' are not part of Add content.
            break;
        }
        if let Some(rest) = text.strip_prefix('+') {
            content_lines.push(rest);
            i += 1;
        } else if text.trim().is_empty() {
            break;
        } else {
            return Err(PublicError::new(
                ErrorCode::MalformedHunk,
                format!(
                    "Add File content lines must start with '+', found: {:?}",
                    text
                ),
            )
            .with_source(SourceSpan {
                line: lines[i].line_no,
                column: 1,
            }));
        }
    }
    let mut content = content_lines.join("\n");
    if !content_lines.is_empty() {
        content.push('\n');
    }
    Ok((content, i))
}

fn parse_update_body(
    lines: &[RawLine<'_>],
    mut i: usize,
) -> Result<(Vec<Hunk>, usize), PublicError> {
    let mut hunks = Vec::new();
    while i < lines.len() {
        let text = lines[i].text;
        if text.starts_with("*** ") {
            break;
        }
        if text.trim().is_empty() {
            i += 1;
            continue;
        }
        if text.starts_with("@@") {
            let span = SourceSpan {
                line: lines[i].line_no,
                column: 1,
            };
            let anchor = parse_hunk_anchor(text);
            i += 1;
            let (hunk_lines, next) = parse_hunk_lines(lines, i)?;
            i = next;
            let mut end_of_file = false;
            if i < lines.len() && lines[i].text == "*** End of File" {
                end_of_file = true;
                i += 1;
            }
            let hunk = Hunk {
                lines: hunk_lines,
                source_span: span,
                anchor,
                end_of_file,
            };
            if !hunk.has_change() {
                return Err(PublicError::new(
                    ErrorCode::MalformedHunk,
                    "Update hunk must contain at least one addition or deletion.",
                )
                .with_source(span));
            }
            hunks.push(hunk);
        } else {
            return Err(PublicError::new(
                ErrorCode::MalformedHunk,
                format!("Expected @@ hunk header, found: {:?}", text),
            )
            .with_source(SourceSpan {
                line: lines[i].line_no,
                column: 1,
            }));
        }
    }
    if hunks.is_empty() {
        return Err(PublicError::new(
            ErrorCode::MalformedHunk,
            "Update File requires at least one hunk.",
        ));
    }
    Ok((hunks, i))
}

/// Parse `@@` / `@@ <anchor>` / unified-diff `@@ -l,s +l,s @@` (numeric → no anchor).
fn parse_hunk_anchor(header: &str) -> Option<String> {
    let rest = header.strip_prefix("@@")?.trim();
    if rest.is_empty() {
        return None;
    }
    // Unified-diff numeric hunk header: starts with -<digits>
    let bytes = rest.as_bytes();
    if bytes.first() == Some(&b'-') && bytes.get(1).map(|b| b.is_ascii_digit()).unwrap_or(false) {
        return None;
    }
    Some(rest.to_string())
}

fn parse_hunk_lines(
    lines: &[RawLine<'_>],
    mut i: usize,
) -> Result<(Vec<HunkLine>, usize), PublicError> {
    let mut hunk_lines = Vec::new();
    while i < lines.len() {
        let text = lines[i].text;
        if text.starts_with("*** ") || text.starts_with("@@") {
            break;
        }
        if text.is_empty() {
            // Empty line inside hunk is treated as context with empty content
            // only if we're still in hunk — but empty lines often separate hunks.
            // Spec: context lines start with space. Empty line ends hunk.
            break;
        }
        let first = text.as_bytes()[0];
        match first {
            b' ' => {
                hunk_lines.push(HunkLine::Context(text[1..].to_string()));
                i += 1;
            }
            b'+' => {
                hunk_lines.push(HunkLine::Add(text[1..].to_string()));
                i += 1;
            }
            b'-' => {
                hunk_lines.push(HunkLine::Delete(text[1..].to_string()));
                i += 1;
            }
            b'\\' => {
                // "\ No newline at end of file" — ignore as advisory
                i += 1;
            }
            _ => {
                return Err(PublicError::new(
                    ErrorCode::MalformedHunk,
                    format!(
                        "Hunk line must start with ' ', '+', or '-', found: {:?}",
                        text
                    ),
                )
                .with_source(SourceSpan {
                    line: lines[i].line_no,
                    column: 1,
                }));
            }
        }
    }
    Ok((hunk_lines, i))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_update() {
        let patch =
            "*** Begin Patch\n*** Update File: a.rs\n@@\n line\n-old\n+new\n*** End Patch\n";
        let doc = parse_patch(patch).unwrap();
        assert_eq!(doc.operations.len(), 1);
        match &doc.operations[0] {
            FileOperation::Update(u) => {
                assert_eq!(u.path, "a.rs");
                assert_eq!(u.hunks.len(), 1);
            }
            _ => panic!("expected update"),
        }
    }

    #[test]
    fn parses_add_delete() {
        let patch = "*** Begin Patch\n*** Add File: n.txt\n+hello\n*** Delete File: old.txt\n*** End Patch\n";
        let doc = parse_patch(patch).unwrap();
        assert_eq!(doc.operations.len(), 2);
    }

    #[test]
    fn rejects_empty() {
        let err = parse_patch("*** Begin Patch\n*** End Patch\n").unwrap_err();
        assert_eq!(err.code, ErrorCode::PatchEmpty);
    }

    #[test]
    fn rejects_missing_begin() {
        let err = parse_patch("*** End Patch\n").unwrap_err();
        assert!(matches!(
            err.code,
            ErrorCode::PatchMissingBegin | ErrorCode::PatchTrailingContent
        ));
    }

    #[test]
    fn parses_end_of_file_marker() {
        let patch = "*** Begin Patch\n*** Update File: a.rs\n@@\n-old\n+new\n*** End of File\n*** End Patch\n";
        let doc = parse_patch(patch).unwrap();
        match &doc.operations[0] {
            FileOperation::Update(u) => {
                assert!(u.hunks[0].end_of_file);
            }
            _ => panic!("expected update"),
        }
    }

    #[test]
    fn parses_anchor_and_ignores_numeric_hunk_header() {
        let patch = "*** Begin Patch\n*** Update File: a.rs\n@@ section\n-old\n+new\n@@ -1,2 +1,2 @@\n-x\n+y\n*** End Patch\n";
        let doc = parse_patch(patch).unwrap();
        match &doc.operations[0] {
            FileOperation::Update(u) => {
                assert_eq!(u.hunks[0].anchor.as_deref(), Some("section"));
                assert_eq!(u.hunks[1].anchor, None);
            }
            _ => panic!("expected update"),
        }
    }

    #[test]
    fn rejects_noop_hunk() {
        let patch = "*** Begin Patch\n*** Update File: a.rs\n@@\n context only\n*** End Patch\n";
        let err = parse_patch(patch).unwrap_err();
        assert_eq!(err.code, ErrorCode::MalformedHunk);
    }
}
