//! Deterministic hunk matching.

use crate::error::{ErrorCode, PublicError, SourceSpan};
use crate::protocol::ast::Hunk;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchRange {
    pub start_line: usize, // 0-based index into file lines
    pub end_line: usize,   // exclusive
}

/// Find a unique match for the hunk's old side against file lines (without newline endings).
pub fn find_unique_match(
    file_lines: &[&str],
    hunk: &Hunk,
    hunk_index: usize,
    path: &str,
) -> Result<MatchRange, PublicError> {
    let old = hunk.old_text_lines();
    if old.is_empty() {
        // Pure insertion: match using surrounding context from new_text...
        // Pure insert hunks have only '+' lines — old is empty.
        // Match position: try to use context lines that appear as Context in the hunk.
        // With only Add lines, insertion location is ambiguous unless we treat as
        // append-only or require context. Spec: insertion with zero old lines —
        // use leading/trailing context from hunk.lines Context entries... but those
        // are in old_text_lines. So pure-add with no context is only valid at EOF
        // if we define it that way. Require at least context OR treat empty old as
        // "insert at every position" which is always ambiguous for non-empty files.
        // Contract: pure addition with no context → match only empty file (start).
        if file_lines.is_empty() {
            return Ok(MatchRange {
                start_line: 0,
                end_line: 0,
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

    // 1. Exact full match
    let matches = find_all_matches(file_lines, &old);
    if matches.len() == 1 {
        return Ok(matches[0]);
    }
    if matches.len() > 1 {
        // Try context reduction
        if let Some(m) = try_context_reduction(file_lines, hunk, &old) {
            return Ok(m);
        }
        return Err(ambiguous(hunk, hunk_index, path));
    }

    // 2/3. Context reduction when zero matches
    if let Some(m) = try_context_reduction(file_lines, hunk, &old) {
        return Ok(m);
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

fn find_all_matches(file_lines: &[&str], needle: &[&str]) -> Vec<MatchRange> {
    let mut out = Vec::new();
    if needle.is_empty() {
        return out;
    }
    if file_lines.len() < needle.len() {
        return out;
    }
    let last = file_lines.len() - needle.len();
    for start in 0..=last {
        if file_lines[start..start + needle.len()] == *needle {
            out.push(MatchRange {
                start_line: start,
                end_line: start + needle.len(),
            });
        }
    }
    out
}

/// Strip leading then trailing context lines until unique match or minimum reached.
fn try_context_reduction(
    file_lines: &[&str],
    hunk: &Hunk,
    _old_full: &[&str],
) -> Option<MatchRange> {
    // Build old lines with classification
    let mut classified: Vec<(bool, &str)> = Vec::new(); // (is_context, text)
    for line in &hunk.lines {
        match line {
            crate::protocol::ast::HunkLine::Context(s) => classified.push((true, s.as_str())),
            crate::protocol::ast::HunkLine::Delete(s) => classified.push((false, s.as_str())),
            crate::protocol::ast::HunkLine::Add(_) => {}
        }
    }

    let mut lead_strip = 0usize;
    let mut trail_strip = 0usize;
    let ctx_count = classified.iter().filter(|(c, _)| *c).count();
    let change_count = classified.iter().filter(|(c, _)| !*c).count();
    if change_count == 0 {
        return None;
    }

    // Iterate strip leading, then trailing, alternating until no more context
    let max_iters = ctx_count + 1;
    for _ in 0..max_iters {
        let needle = build_reduced(&classified, lead_strip, trail_strip)?;
        // Minimum: at least one old line remaining
        if needle.is_empty() {
            return None;
        }
        let matches = find_all_matches(file_lines, &needle);
        if matches.len() == 1 {
            return Some(matches[0]);
        }
        if matches.len() > 1 {
            // Need more reduction
        } else {
            // zero — keep reducing
        }

        // Prefer strip one leading context, else trailing
        let can_lead = lead_strip < classified.len()
            && classified.get(lead_strip).map(|(c, _)| *c).unwrap_or(false);
        // After lead_strip, first remaining should be context to strip
        let first_ctx_idx = classified
            .iter()
            .enumerate()
            .skip(lead_strip)
            .find_map(|(i, (c, _))| if *c { Some(i) } else { None });
        let last_ctx_idx = classified
            .iter()
            .enumerate()
            .rev()
            .skip(trail_strip)
            .find_map(|(i, (c, _))| {
                if *c && i + 1 + trail_strip <= classified.len() {
                    Some(i)
                } else {
                    None
                }
            });

        if let Some(idx) = first_ctx_idx {
            if idx == lead_strip || classified[lead_strip].0 {
                // strip next leading context by increasing lead_strip to idx+1 if context at start
                if can_lead || first_ctx_idx == Some(lead_strip) {
                    lead_strip = idx + 1;
                    continue;
                }
            }
        }
        if let Some(idx) = last_ctx_idx {
            let from_end = classified.len() - 1 - idx;
            if from_end >= trail_strip {
                trail_strip = from_end + 1;
                continue;
            }
        }
        break;
    }
    None
}

fn build_reduced<'a>(
    classified: &[(bool, &'a str)],
    lead_strip: usize,
    trail_strip: usize,
) -> Option<Vec<&'a str>> {
    if lead_strip + trail_strip >= classified.len() {
        return None;
    }
    let slice = &classified[lead_strip..classified.len() - trail_strip];
    // Must retain all non-context (delete) lines
    if !classified.iter().filter(|(c, _)| !*c).all(|(c, text)| {
        !*c && slice.iter().any(|(sc, st)| !sc && st == text)
            || slice.iter().any(|x| x == &(*c, *text))
    }) {
        // Simpler: ensure all delete lines are still in slice
    }
    let deletes_ok = classified
        .iter()
        .filter(|(c, _)| !*c)
        .all(|d| slice.iter().any(|s| s == d));
    if !deletes_ok {
        return None;
    }
    Some(slice.iter().map(|(_, t)| *t).collect())
}

/// Apply a single hunk at a known range, returning new file lines.
pub fn apply_at(
    file_lines: &[&str],
    range: MatchRange,
    hunk: &Hunk,
) -> Result<Vec<String>, PublicError> {
    let new_side: Vec<String> = hunk
        .new_text_lines()
        .into_iter()
        .map(str::to_string)
        .collect();
    let mut out = Vec::with_capacity(file_lines.len() + new_side.len());
    out.extend(
        file_lines[..range.start_line]
            .iter()
            .map(|s| (*s).to_string()),
    );
    out.extend(new_side);
    out.extend(
        file_lines[range.end_line..]
            .iter()
            .map(|s| (*s).to_string()),
    );
    Ok(out)
}

#[allow(dead_code)]
fn _span(hunk: &Hunk) -> SourceSpan {
    hunk.source_span
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::ast::{Hunk, HunkLine};

    fn hunk(lines: Vec<HunkLine>) -> Hunk {
        Hunk {
            lines,
            source_span: SourceSpan { line: 1, column: 1 },
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
        let m = find_unique_match(&file, &h, 0, "f").unwrap();
        assert_eq!(m.start_line, 1);
        assert_eq!(m.end_line, 3);
    }

    #[test]
    fn ambiguous() {
        let file = ["x", "y", "x", "y"];
        let h = hunk(vec![
            HunkLine::Delete("x".into()),
            HunkLine::Add("z".into()),
        ]);
        let err = find_unique_match(&file, &h, 0, "f").unwrap_err();
        assert_eq!(err.code, ErrorCode::HunkAmbiguous);
    }

    #[test]
    fn not_found() {
        let file = ["a"];
        let h = hunk(vec![
            HunkLine::Delete("missing".into()),
            HunkLine::Add("x".into()),
        ]);
        let err = find_unique_match(&file, &h, 0, "f").unwrap_err();
        assert_eq!(err.code, ErrorCode::HunkNotFound);
    }
}
