//! In-memory patch application (locate-all → forward emit).

use super::diff_summary::{diff_line_counts, DiffCounts};
use super::emit::emit_chunks;
use super::locate::locate_chunks_with;
use crate::error::{ErrorCode, PublicError};
use crate::match_opts::MatchOptions;
use crate::protocol::ast::UpdateFile;

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
    apply_update_with(
        base,
        update,
        newline,
        final_newline,
        MatchOptions::default(),
    )
}

pub fn apply_update_with(
    base: &str,
    update: &UpdateFile,
    newline: &str,
    final_newline: bool,
    opts: MatchOptions,
) -> Result<AppliedText, PublicError> {
    let lines = split_content_lines(base);
    let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
    let chunks = locate_chunks_with(&line_refs, update, opts)?;
    let new_lines = emit_chunks(&line_refs, &chunks, &update.path)?;

    let text = join_lines(&new_lines, newline, final_newline);
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
        return String::new();
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

    fn update(hunks: Vec<Hunk>) -> UpdateFile {
        UpdateFile {
            path: "f".into(),
            source_span: SourceSpan { line: 1, column: 1 },
            hunks,
            hash_pin: None,
        }
    }

    fn hunk(anchor: Option<&str>, lines: Vec<HunkLine>) -> Hunk {
        Hunk {
            lines,
            source_span: SourceSpan { line: 2, column: 1 },
            anchor: anchor.map(str::to_string),
            end_of_file: false,
        }
    }

    #[test]
    fn applies_simple_replace() {
        let base = "a\nb\nc\n";
        let applied = apply_update(
            base,
            &update(vec![hunk(
                None,
                vec![
                    HunkLine::Context("b".into()),
                    HunkLine::Delete("c".into()),
                    HunkLine::Add("d".into()),
                ],
            )]),
            "\n",
            true,
        )
        .unwrap();
        assert_eq!(applied.text, "a\nb\nd\n");
    }

    #[test]
    fn crlf_file_lf_patch_preserves_crlf() {
        let base = "line1\r\nline2\r\nline3\r\n";
        let applied = apply_update(
            base,
            &update(vec![hunk(
                Some("line1"),
                vec![
                    HunkLine::Delete("line2".into()),
                    HunkLine::Add("updated".into()),
                    HunkLine::Context("line3".into()),
                ],
            )]),
            "\r\n",
            true,
        )
        .unwrap();
        assert_eq!(applied.text, "line1\r\nupdated\r\nline3\r\n");
    }

    #[test]
    fn lf_file_crlf_patch_preserves_lf() {
        let base = "line1\nline2\nline3\n";
        let applied = apply_update(
            base,
            &update(vec![hunk(
                Some("line1"),
                vec![
                    HunkLine::Delete("line2".into()),
                    HunkLine::Add("updated".into()),
                    HunkLine::Context("line3".into()),
                ],
            )]),
            "\n",
            true,
        )
        .unwrap();
        assert_eq!(applied.text, "line1\nupdated\nline3\n");
    }

    #[test]
    fn crlf_file_crlf_patch_preserves_crlf() {
        let base = "line1\r\nline2\r\nline3\r\n";
        let applied = apply_update(
            base,
            &update(vec![hunk(
                None,
                vec![
                    HunkLine::Context("line1".into()),
                    HunkLine::Delete("line2".into()),
                    HunkLine::Add("updated".into()),
                    HunkLine::Context("line3".into()),
                ],
            )]),
            "\r\n",
            true,
        )
        .unwrap();
        assert_eq!(applied.text, "line1\r\nupdated\r\nline3\r\n");
    }

    #[test]
    fn multi_hunk_locate_then_emit_on_original() {
        let base = "a\nb\nc\nd\n";
        let applied = apply_update(
            base,
            &update(vec![
                hunk(
                    None,
                    vec![HunkLine::Delete("b".into()), HunkLine::Add("B".into())],
                ),
                hunk(
                    None,
                    vec![HunkLine::Delete("d".into()), HunkLine::Add("D".into())],
                ),
            ]),
            "\n",
            true,
        )
        .unwrap();
        assert_eq!(applied.text, "a\nB\nc\nD\n");
    }

    #[test]
    fn detect_crlf_and_lf() {
        assert!(matches!(
            detect_newline_style("a\r\nb\r\n"),
            crate::error::NewlineStyle::CrLf
        ));
        assert!(matches!(
            detect_newline_style("a\nb\n"),
            crate::error::NewlineStyle::Lf
        ));
        assert!(matches!(
            detect_newline_style("a\r\nb\nc"),
            crate::error::NewlineStyle::Mixed
        ));
    }
}
