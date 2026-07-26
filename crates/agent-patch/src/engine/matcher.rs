//! Deterministic unique-exact hunk matching (with optional context reduction).

use crate::error::{ErrorCode, PublicError};
use crate::protocol::ast::{Hunk, HunkLine};

/// Inclusive-exclusive range plus the insertion lines to emit for that hit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchHit {
    pub start_line: usize,
    pub end_line: usize,
    pub ins_lines: Vec<String>,
}

/// Find a unique match for the hunk's old side in `file_lines[search_start..]`.
///
/// When `prefer_eof` is set (`*** End of File`), try an exact match aligned at the
/// end of the file first (Codex/Agents EOF prefer). If that fails, fall back to the
/// normal unique-exact forward search from `search_start`.
///
/// When context reduction is used, `ins_lines` are reduced by the same leading/trailing
/// context strips so emit replaces exactly the matched span.
pub fn find_unique_match(
    file_lines: &[&str],
    hunk: &Hunk,
    hunk_index: usize,
    path: &str,
    search_start: usize,
    prefer_eof: bool,
) -> Result<MatchHit, PublicError> {
    let old = hunk.old_text_lines();
    let full_ins: Vec<String> = hunk
        .new_text_lines()
        .into_iter()
        .map(str::to_string)
        .collect();

    if old.is_empty() {
        if prefer_eof {
            // Pure insertion at EOF: append at end of file.
            return Ok(MatchHit {
                start_line: file_lines.len(),
                end_line: file_lines.len(),
                ins_lines: full_ins,
            });
        }
        if file_lines.is_empty() && search_start == 0 {
            return Ok(MatchHit {
                start_line: 0,
                end_line: 0,
                ins_lines: full_ins,
            });
        }
        return Err(PublicError::new(
            ErrorCode::HunkAmbiguous,
            format!(
                "Update hunk {} is a pure insertion without context and cannot uniquely locate a position.",
                hunk_index + 1
            ),
        )
        .with_path(path)
        .with_hunk(hunk_index)
        .with_source(hunk.source_span));
    }

    if prefer_eof {
        if let Some(hit) = match_at_eof(file_lines, &old, &full_ins, search_start) {
            return Ok(hit);
        }
        if let Some(hit) = try_context_reduction_at_eof(file_lines, hunk, search_start) {
            return Ok(hit);
        }
        // Fall back to unique forward search (Agents EOF fallback, but unique-exact).
    }

    let matches = find_all_matches(file_lines, &old, search_start);
    if matches.len() == 1 {
        let (start, end) = matches[0];
        return Ok(MatchHit {
            start_line: start,
            end_line: end,
            ins_lines: full_ins,
        });
    }
    if matches.len() > 1 {
        if let Some(hit) = try_context_reduction(file_lines, hunk, search_start) {
            return Ok(hit);
        }
        return Err(ambiguous(hunk, hunk_index, path));
    }

    if let Some(hit) = try_context_reduction(file_lines, hunk, search_start) {
        return Ok(hit);
    }

    Err(PublicError::new(
        ErrorCode::HunkNotFound,
        format!(
            "Update hunk {} did not match the current file.",
            hunk_index + 1
        ),
    )
    .with_path(path)
    .with_hunk(hunk_index)
    .with_source(hunk.source_span)
    .with_hint("Read the current affected region and regenerate the patch from current content."))
}

fn match_at_eof(
    file_lines: &[&str],
    old: &[&str],
    ins_lines: &[String],
    search_start: usize,
) -> Option<MatchHit> {
    if file_lines.len() < old.len() {
        return None;
    }
    let start = file_lines.len() - old.len();
    if start < search_start {
        return None;
    }
    if file_lines[start..start + old.len()] == *old {
        return Some(MatchHit {
            start_line: start,
            end_line: start + old.len(),
            ins_lines: ins_lines.to_vec(),
        });
    }
    None
}

fn try_context_reduction_at_eof(
    file_lines: &[&str],
    hunk: &Hunk,
    search_start: usize,
) -> Option<MatchHit> {
    let old_entries = old_side_entries(hunk);
    let change_count = old_entries.iter().filter(|(_, is_ctx, _)| !*is_ctx).count();
    if change_count == 0 {
        return None;
    }
    let ctx_count = old_entries.iter().filter(|(_, is_ctx, _)| *is_ctx).count();
    let mut lead = 0usize;
    let mut trail = 0usize;
    let mut prefer_lead = true;
    for _ in 0..ctx_count {
        let can_lead = can_strip_leading(&old_entries, lead, trail);
        let can_trail = can_strip_trailing(&old_entries, lead, trail);
        if prefer_lead && can_lead {
            lead += 1;
            prefer_lead = false;
        } else if can_trail {
            trail += 1;
            prefer_lead = true;
        } else if can_lead {
            lead += 1;
            prefer_lead = false;
        } else {
            break;
        }
        let Some(slice) = strip_old_edges(&old_entries, lead, trail) else {
            break;
        };
        let needle: Vec<&str> = slice.iter().map(|(_, _, t)| *t).collect();
        let Some(ins) = reduce_new_side(hunk, lead, trail) else {
            break;
        };
        if let Some(hit) = match_at_eof(file_lines, &needle, &ins, search_start) {
            return Some(hit);
        }
    }
    None
}

fn ambiguous(hunk: &Hunk, hunk_index: usize, path: &str) -> PublicError {
    PublicError::new(
        ErrorCode::HunkAmbiguous,
        format!(
            "Update hunk {} matched multiple locations in the current file.",
            hunk_index + 1
        ),
    )
    .with_path(path)
    .with_hunk(hunk_index)
    .with_source(hunk.source_span)
    .with_hint("Add more unique surrounding context and regenerate the patch.")
}

fn find_all_matches(
    file_lines: &[&str],
    needle: &[&str],
    search_start: usize,
) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    if needle.is_empty() {
        return out;
    }
    if search_start >= file_lines.len() {
        return out;
    }
    if file_lines.len() < search_start + needle.len() {
        return out;
    }
    let last = file_lines.len() - needle.len();
    for start in search_start..=last {
        if file_lines[start..start + needle.len()] == *needle {
            out.push((start, start + needle.len()));
        }
    }
    out
}

/// Strip leading then trailing context until a unique match, or give up.
fn try_context_reduction(
    file_lines: &[&str],
    hunk: &Hunk,
    search_start: usize,
) -> Option<MatchHit> {
    let old_entries = old_side_entries(hunk);
    let change_count = old_entries.iter().filter(|(_, is_ctx, _)| !*is_ctx).count();
    if change_count == 0 {
        return None;
    }

    let ctx_count = old_entries.iter().filter(|(_, is_ctx, _)| *is_ctx).count();
    let mut lead = 0usize;
    let mut trail = 0usize;
    let mut prefer_lead = true;

    // Contract: strip one leading context, then one trailing, alternating; accept only if unique.
    for _ in 0..ctx_count {
        let can_lead = can_strip_leading(&old_entries, lead, trail);
        let can_trail = can_strip_trailing(&old_entries, lead, trail);
        if prefer_lead && can_lead {
            lead += 1;
            prefer_lead = false;
        } else if can_trail {
            trail += 1;
            prefer_lead = true;
        } else if can_lead {
            lead += 1;
            prefer_lead = false;
        } else {
            break;
        }
        if let Some(hit) = match_reduced(file_lines, hunk, &old_entries, lead, trail, search_start)
        {
            return Some(hit);
        }
    }
    None
}

fn match_reduced(
    file_lines: &[&str],
    hunk: &Hunk,
    old_entries: &[(usize, bool, &str)],
    lead: usize,
    trail: usize,
    search_start: usize,
) -> Option<MatchHit> {
    if lead == 0 && trail == 0 {
        return None;
    }
    let slice = strip_old_edges(old_entries, lead, trail)?;
    let needle: Vec<&str> = slice.iter().map(|(_, _, t)| *t).collect();
    if needle.is_empty() {
        return None;
    }
    let matches = find_all_matches(file_lines, &needle, search_start);
    if matches.len() != 1 {
        return None;
    }
    let (start, end) = matches[0];
    let ins_lines = reduce_new_side(hunk, lead, trail)?;
    Some(MatchHit {
        start_line: start,
        end_line: end,
        ins_lines,
    })
}

fn old_side_entries(hunk: &Hunk) -> Vec<(usize, bool, &str)> {
    hunk.lines
        .iter()
        .enumerate()
        .filter_map(|(i, line)| match line {
            HunkLine::Context(s) => Some((i, true, s.as_str())),
            HunkLine::Delete(s) => Some((i, false, s.as_str())),
            HunkLine::Add(_) => None,
        })
        .collect()
}

fn strip_old_edges<'a>(
    old_entries: &'a [(usize, bool, &'a str)],
    lead: usize,
    trail: usize,
) -> Option<&'a [(usize, bool, &'a str)]> {
    let mut lo = 0usize;
    let mut hi = old_entries.len();
    for _ in 0..lead {
        if lo >= hi || !old_entries[lo].1 {
            return None;
        }
        lo += 1;
    }
    for _ in 0..trail {
        if lo >= hi || !old_entries[hi - 1].1 {
            return None;
        }
        hi -= 1;
    }
    if lo >= hi {
        return None;
    }
    let slice = &old_entries[lo..hi];
    let deletes_ok = old_entries
        .iter()
        .filter(|(_, is_ctx, _)| !*is_ctx)
        .all(|d| slice.iter().any(|s| s.0 == d.0));
    if !deletes_ok {
        return None;
    }
    Some(slice)
}

fn can_strip_leading(old_entries: &[(usize, bool, &str)], lead: usize, trail: usize) -> bool {
    strip_old_edges(old_entries, lead + 1, trail).is_some()
}

fn can_strip_trailing(old_entries: &[(usize, bool, &str)], lead: usize, trail: usize) -> bool {
    strip_old_edges(old_entries, lead, trail + 1).is_some()
}

fn reduce_new_side(hunk: &Hunk, lead: usize, trail: usize) -> Option<Vec<String>> {
    let mut entries: Vec<(bool, String)> = Vec::new();
    for line in &hunk.lines {
        match line {
            HunkLine::Context(s) => entries.push((true, s.clone())),
            HunkLine::Add(s) => entries.push((false, s.clone())),
            HunkLine::Delete(_) => {}
        }
    }

    let mut lo = 0usize;
    let mut hi = entries.len();
    for _ in 0..lead {
        if lo >= hi || !entries[lo].0 {
            return None;
        }
        lo += 1;
    }
    for _ in 0..trail {
        if lo >= hi || !entries[hi - 1].0 {
            return None;
        }
        hi -= 1;
    }
    if lo > hi {
        return None;
    }
    Some(entries[lo..hi].iter().map(|(_, s)| s.clone()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::SourceSpan;

    fn hunk(lines: Vec<HunkLine>) -> Hunk {
        Hunk {
            lines,
            source_span: SourceSpan { line: 1, column: 1 },
            anchor: None,
            end_of_file: false,
        }
    }

    #[test]
    fn exact_match() {
        let file = ["a", "b", "c"];
        let h = hunk(vec![
            HunkLine::Context("b".into()),
            HunkLine::Delete("c".into()),
            HunkLine::Add("d".into()),
        ]);
        let m = find_unique_match(&file, &h, 0, "f", 0, false).unwrap();
        assert_eq!(m.start_line, 1);
        assert_eq!(m.end_line, 3);
        assert_eq!(m.ins_lines, vec!["b".to_string(), "d".to_string()]);
    }

    #[test]
    fn ambiguous() {
        let file = ["x", "y", "x", "y"];
        let h = hunk(vec![
            HunkLine::Delete("x".into()),
            HunkLine::Add("z".into()),
        ]);
        let err = find_unique_match(&file, &h, 0, "f", 0, false).unwrap_err();
        assert_eq!(err.code, ErrorCode::HunkAmbiguous);
    }

    #[test]
    fn not_found() {
        let file = ["a"];
        let h = hunk(vec![
            HunkLine::Delete("missing".into()),
            HunkLine::Add("x".into()),
        ]);
        let err = find_unique_match(&file, &h, 0, "f", 0, false).unwrap_err();
        assert_eq!(err.code, ErrorCode::HunkNotFound);
    }

    #[test]
    fn search_start_skips_earlier_match() {
        let file = ["x", "a", "x", "b"];
        let h = hunk(vec![
            HunkLine::Delete("x".into()),
            HunkLine::Add("X".into()),
        ]);
        let m = find_unique_match(&file, &h, 0, "f", 2, false).unwrap();
        assert_eq!(m.start_line, 2);
    }

    #[test]
    fn context_reduction_unique_core_adjusts_ins() {
        // Full [shared, target] is absent; reduced [target] after stripping leading context is unique.
        let file = ["pad", "target", "shared"];
        let h = hunk(vec![
            HunkLine::Context("shared".into()),
            HunkLine::Delete("target".into()),
            HunkLine::Add("done".into()),
        ]);
        let m = find_unique_match(&file, &h, 0, "f", 0, false).unwrap();
        assert_eq!(m.start_line, 1);
        assert_eq!(m.end_line, 2);
        assert_eq!(m.ins_lines, vec!["done".to_string()]);
    }

    #[test]
    fn context_reduction_still_ambiguous_fails() {
        let file = ["ctx", "target", "ctx", "target"];
        let h = hunk(vec![
            HunkLine::Context("ctx".into()),
            HunkLine::Delete("target".into()),
            HunkLine::Add("done".into()),
        ]);
        let err = find_unique_match(&file, &h, 0, "f", 0, false).unwrap_err();
        assert_eq!(err.code, ErrorCode::HunkAmbiguous);
    }

    #[test]
    fn eof_prefer_picks_trailing_duplicate() {
        let file = ["first", "second", "first", "second"];
        let h = Hunk {
            lines: vec![
                HunkLine::Context("first".into()),
                HunkLine::Delete("second".into()),
                HunkLine::Add("second updated".into()),
            ],
            source_span: SourceSpan { line: 1, column: 1 },
            anchor: None,
            end_of_file: true,
        };
        let m = find_unique_match(&file, &h, 0, "f", 0, true).unwrap();
        assert_eq!(m.start_line, 2);
        assert_eq!(
            m.ins_lines,
            vec!["first".to_string(), "second updated".to_string()]
        );
    }
}
