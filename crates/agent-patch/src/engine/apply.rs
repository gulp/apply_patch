//! In-memory patch application.

use super::diff_summary::{diff_line_counts, DiffCounts};
use super::matcher::{apply_at, find_unique_match, MatchRange};
use crate::error::{ErrorCode, PublicError};
use crate::protocol::ast::{Hunk, UpdateFile};

#[derive(Debug, Clone)]
pub struct AppliedText {
    pub text: String,
    pub counts: DiffCounts,
    pub hunks_applied: usize,
}

/// Apply all update hunks to base text. `newline` is the line ending to use when rejoining
/// (`\n` or `\r\n`). `final_newline` preserves whether the original ended with a newline.
pub fn apply_update(
    base: &str,
    update: &UpdateFile,
    newline: &str,
    final_newline: bool,
) -> Result<AppliedText, PublicError> {
    let mut lines: Vec<String> = split_content_lines(base);
    let mut occupied: Vec<(usize, usize)> = Vec::new(); // ranges in original line space — track via sequential apply

    for (hunk_index, hunk) in update.hunks.iter().enumerate() {
        let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
        let range = find_unique_match(&line_refs, hunk, hunk_index, &update.path)?;
        check_overlap(&occupied, range, hunk, hunk_index, &update.path)?;
        // Record occupied in current line coordinates before mutation; after apply,
        // subsequent matches search the updated buffer so overlaps are on current coords.
        let new_lines = apply_at(&line_refs, range, hunk)?;
        let new_len = hunk.new_text_lines().len();
        // Update occupied ranges for subsequent overlap checks in the new coordinate space.
        // Simpler approach: map occupied ranges through the edit.
        let mut next_occupied = Vec::new();
        for (s, e) in occupied {
            if e <= range.start_line {
                next_occupied.push((s, e));
            } else if s >= range.end_line {
                let delta =
                    new_len as isize - (range.end_line as isize - range.start_line as isize);
                next_occupied.push(((s as isize + delta) as usize, (e as isize + delta) as usize));
            } else {
                return Err(overlap_err(hunk, hunk_index, &update.path));
            }
        }
        next_occupied.push((range.start_line, range.start_line + new_len));
        occupied = next_occupied;
        lines = new_lines;
    }

    let text = join_lines(&lines, newline, final_newline);
    if text == base {
        return Err(PublicError::new(
            ErrorCode::PatchNoEffect,
            format!("Update of {} produced no content changes.", update.path),
        )
        .with_path(&update.path));
    }
    let counts = diff_line_counts(base, &text);
    Ok(AppliedText {
        text,
        counts,
        hunks_applied: update.hunks.len(),
    })
}

fn overlap_err(hunk: &Hunk, hunk_index: usize, path: &str) -> PublicError {
    PublicError::new(
        ErrorCode::HunkOverlap,
        format!(
            "Update hunk {} overlaps a previously applied hunk.",
            hunk_index + 1
        ),
    )
    .with_path(path)
    .with_hunk(hunk_index)
    .with_source(hunk.source_span)
}

fn check_overlap(
    occupied: &[(usize, usize)],
    range: MatchRange,
    hunk: &Hunk,
    hunk_index: usize,
    path: &str,
) -> Result<(), PublicError> {
    for &(s, e) in occupied {
        if range.start_line < e && s < range.end_line {
            return Err(overlap_err(hunk, hunk_index, path));
        }
    }
    Ok(())
}

/// Split file content into lines without endings. Preserves whether final newline existed separately.
pub fn split_content_lines(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut lines = Vec::new();
    let mut start = 0usize;
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'\n' {
            let end = if i > start && bytes[i - 1] == b'\r' {
                i - 1
            } else {
                i
            };
            lines.push(text[start..end].to_string());
            i += 1;
            start = i;
        } else {
            i += 1;
        }
    }
    if start < text.len() {
        // No trailing newline — last line
        let end = if text[start..].ends_with('\r') {
            text.len() - 1
        } else {
            text.len()
        };
        lines.push(text[start..end].to_string());
    }
    // If text ends with newline, start == len and we don't push an extra empty line.
    lines
}

pub fn join_lines(lines: &[String], newline: &str, final_newline: bool) -> String {
    if lines.is_empty() {
        return if final_newline {
            // empty file with final newline is just empty string conventionally;
            // an empty file never has content. final_newline on empty → ""
            String::new()
        } else {
            String::new()
        };
    }
    let mut out = lines.join(newline);
    if final_newline {
        out.push_str(newline);
    }
    out
}

pub fn detect_newline_style(text: &str) -> crate::error::NewlineStyle {
    use crate::error::NewlineStyle;
    if text.is_empty() {
        return NewlineStyle::None;
    }
    let has_crlf = text.contains("\r\n");
    let lf_only = {
        let without_crlf = text.replace("\r\n", "");
        without_crlf.contains('\n')
    };
    let has_bare_cr = text.replace("\r\n", "").contains('\r');
    match (has_crlf, lf_only, has_bare_cr) {
        (false, false, false) => NewlineStyle::None,
        (true, false, false) => NewlineStyle::CrLf,
        (false, true, false) => NewlineStyle::Lf,
        (false, false, true) => NewlineStyle::Mixed, // bare CR treated as mixed/unsupported
        _ => NewlineStyle::Mixed,
    }
}

pub fn has_final_newline(text: &str) -> bool {
    text.ends_with('\n')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::SourceSpan;
    use crate::protocol::ast::{Hunk, HunkLine, UpdateFile};

    #[test]
    fn applies_simple_replace() {
        let base = "a\nb\nc\n";
        let update = UpdateFile {
            path: "f".into(),
            source_span: SourceSpan { line: 1, column: 1 },
            hunks: vec![Hunk {
                source_span: SourceSpan { line: 2, column: 1 },
                lines: vec![
                    HunkLine::Context("b".into()),
                    HunkLine::Delete("c".into()),
                    HunkLine::Add("d".into()),
                ],
            }],
        };
        let applied = apply_update(base, &update, "\n", true).unwrap();
        assert_eq!(applied.text, "a\nb\nd\n");
    }
}
