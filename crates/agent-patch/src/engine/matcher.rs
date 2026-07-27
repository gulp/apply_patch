//! Deterministic unique-exact hunk matching (with optional context reduction).

use crate::error::{ErrorCode, PublicError};
use crate::match_opts::{normalize_line, FuzzyMode};
use crate::oracle::{build_candidates, draft_repair_patch, MatchEvidence, MatchLevel};
use crate::protocol::ast::{Hunk, HunkLine};

/// Inclusive-exclusive range plus the insertion lines to emit for that hit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchHit {
    pub start_line: usize,
    pub end_line: usize,
    pub ins_lines: Vec<String>,
    pub evidence: MatchEvidence,
}

/// Find a unique match for the hunk's old side in `file_lines[search_start..]`.
///
/// When `prefer_eof` is set (`*** End of File`), try an exact match aligned at the
/// end of the file first (Codex/Agents EOF prefer). If that fails, fall back to the
/// normal unique-exact forward search from `search_start`.
///
/// When context reduction is used, `ins_lines` are reduced by the same leading/trailing
/// context strips so emit replaces exactly the matched span.
#[allow(clippy::too_many_arguments)]
pub fn find_unique_match(
    file_lines: &[&str],
    hunk: &Hunk,
    hunk_index: usize,
    path: &str,
    search_start: usize,
    prefer_eof: bool,
    used_anchor: bool,
    fuzzy: FuzzyMode,
) -> Result<MatchHit, PublicError> {
    let old = hunk.old_text_lines();
    let full_ins: Vec<String> = hunk
        .new_text_lines()
        .into_iter()
        .map(str::to_string)
        .collect();

    let base_ev =
        |level: MatchLevel, count: usize, lead: usize, trail: usize, twins: usize| MatchEvidence {
            path: path.to_string(),
            hunk_index,
            accepted_level: level,
            candidate_count: count,
            retained_context_lead: lead,
            retained_context_trail: trail,
            used_anchor,
            used_eof: prefer_eof,
            nearby_twins: twins,
        };

    if old.is_empty() {
        if prefer_eof {
            return Ok(MatchHit {
                start_line: file_lines.len(),
                end_line: file_lines.len(),
                ins_lines: full_ins,
                evidence: base_ev(MatchLevel::Eof, 1, 0, 0, 0),
            });
        }
        if file_lines.is_empty() && search_start == 0 {
            return Ok(MatchHit {
                start_line: 0,
                end_line: 0,
                ins_lines: full_ins,
                evidence: base_ev(MatchLevel::Exact, 1, 0, 0, 0),
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
            return Ok(MatchHit {
                start_line: hit.0,
                end_line: hit.1,
                ins_lines: hit.2,
                evidence: base_ev(MatchLevel::Eof, 1, 0, 0, 0),
            });
        }
        if let Some((hit, lead, trail)) =
            try_context_reduction_at_eof(file_lines, hunk, search_start)
        {
            return Ok(MatchHit {
                start_line: hit.0,
                end_line: hit.1,
                ins_lines: hit.2,
                evidence: base_ev(MatchLevel::Eof, 1, lead, trail, 0),
            });
        }
    }

    let matches = find_all_matches(file_lines, &old, search_start);
    if matches.len() == 1 {
        let (start, end) = matches[0];
        return Ok(MatchHit {
            start_line: start,
            end_line: end,
            ins_lines: full_ins,
            evidence: base_ev(MatchLevel::Exact, 1, 0, 0, 0),
        });
    }
    if matches.len() > 1 {
        if let Some((hit, lead, trail)) = try_context_reduction(file_lines, hunk, search_start) {
            return Ok(MatchHit {
                start_line: hit.start_line,
                end_line: hit.end_line,
                ins_lines: hit.ins_lines,
                evidence: base_ev(
                    MatchLevel::ContextReduced,
                    1,
                    lead,
                    trail,
                    matches.len().saturating_sub(1),
                ),
            });
        }
        return Err(ambiguous_with_candidates(
            hunk, hunk_index, path, file_lines, &matches,
        ));
    }

    if let Some((hit, lead, trail)) = try_context_reduction(file_lines, hunk, search_start) {
        return Ok(MatchHit {
            start_line: hit.start_line,
            end_line: hit.end_line,
            ins_lines: hit.ins_lines,
            evidence: base_ev(MatchLevel::ContextReduced, 1, lead, trail, 0),
        });
    }

    // Unique-only fuzzy ladder (never first-match).
    if fuzzy.allows_rstrip() {
        let norm = FuzzyMode::Rstrip;
        let matches = find_all_matches_normalized(file_lines, &old, search_start, norm);
        if matches.len() == 1 {
            let (start, end) = matches[0];
            return Ok(MatchHit {
                start_line: start,
                end_line: end,
                ins_lines: full_ins,
                evidence: base_ev(MatchLevel::Rstrip, 1, 0, 0, 0),
            });
        }
        if matches.len() > 1 {
            return Err(ambiguous_with_candidates(
                hunk, hunk_index, path, file_lines, &matches,
            ));
        }
    }
    if fuzzy.allows_strip() {
        let norm = FuzzyMode::Strip;
        let matches = find_all_matches_normalized(file_lines, &old, search_start, norm);
        if matches.len() == 1 {
            let (start, end) = matches[0];
            return Ok(MatchHit {
                start_line: start,
                end_line: end,
                ins_lines: full_ins,
                evidence: base_ev(MatchLevel::Strip, 1, 0, 0, 0),
            });
        }
        if matches.len() > 1 {
            return Err(ambiguous_with_candidates(
                hunk, hunk_index, path, file_lines, &matches,
            ));
        }
    }

    let mut ranges = matches;
    if ranges.is_empty() {
        if let Some(first) = old.first() {
            for (i, line) in file_lines.iter().enumerate().skip(search_start) {
                if line == first {
                    let end = (i + old.len()).min(file_lines.len());
                    ranges.push((i, end));
                    if ranges.len() >= crate::oracle::MAX_CANDIDATES {
                        break;
                    }
                }
            }
        }
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
    .with_hint("Read the current affected region and regenerate the patch from current content.")
    .with_candidates(build_candidates(file_lines, &ranges))
    .maybe_repair(path, file_lines, hunk, &ranges))
}

fn match_at_eof(
    file_lines: &[&str],
    old: &[&str],
    ins_lines: &[String],
    search_start: usize,
) -> Option<(usize, usize, Vec<String>)> {
    if file_lines.len() < old.len() {
        return None;
    }
    let start = file_lines.len() - old.len();
    if start < search_start {
        return None;
    }
    if file_lines[start..start + old.len()] == *old {
        return Some((start, start + old.len(), ins_lines.to_vec()));
    }
    None
}

#[allow(clippy::type_complexity)]
fn try_context_reduction_at_eof(
    file_lines: &[&str],
    hunk: &Hunk,
    search_start: usize,
) -> Option<((usize, usize, Vec<String>), usize, usize)> {
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
            return Some((hit, lead, trail));
        }
    }
    None
}

fn ambiguous_with_candidates(
    hunk: &Hunk,
    hunk_index: usize,
    path: &str,
    file_lines: &[&str],
    matches: &[(usize, usize)],
) -> PublicError {
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
    .with_candidates(build_candidates(file_lines, matches))
    .maybe_repair(path, file_lines, hunk, matches)
}

trait MaybeRepair {
    fn maybe_repair(
        self,
        path: &str,
        file_lines: &[&str],
        hunk: &Hunk,
        ranges: &[(usize, usize)],
    ) -> PublicError;
}

impl MaybeRepair for PublicError {
    fn maybe_repair(
        self,
        path: &str,
        file_lines: &[&str],
        hunk: &Hunk,
        ranges: &[(usize, usize)],
    ) -> PublicError {
        match draft_repair_patch(path, file_lines, &hunk.new_text_lines(), ranges) {
            Some(repair) => self.with_repair_patch(repair),
            None => self,
        }
    }
}

pub fn find_all_matches(
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

pub fn find_all_matches_normalized(
    file_lines: &[&str],
    needle: &[&str],
    search_start: usize,
    mode: FuzzyMode,
) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    if needle.is_empty() || mode == FuzzyMode::Off {
        return out;
    }
    if search_start >= file_lines.len() {
        return out;
    }
    if file_lines.len() < search_start + needle.len() {
        return out;
    }
    let needle_n: Vec<String> = needle.iter().map(|l| normalize_line(l, mode)).collect();
    let last = file_lines.len() - needle.len();
    for start in search_start..=last {
        let ok = file_lines[start..start + needle.len()]
            .iter()
            .zip(needle_n.iter())
            .all(|(line, n)| normalize_line(line, mode) == *n);
        if ok {
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
) -> Option<(MatchHit, usize, usize)> {
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
        if let Some(hit) = match_reduced(file_lines, hunk, &old_entries, lead, trail, search_start)
        {
            return Some((hit, lead, trail));
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
        evidence: MatchEvidence {
            path: String::new(),
            hunk_index: 0,
            accepted_level: MatchLevel::ContextReduced,
            candidate_count: 1,
            retained_context_lead: lead,
            retained_context_trail: trail,
            used_anchor: false,
            used_eof: false,
            nearby_twins: 0,
        },
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
        let m = find_unique_match(&file, &h, 0, "f", 0, false, false, FuzzyMode::Off).unwrap();
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
        let err =
            find_unique_match(&file, &h, 0, "f", 0, false, false, FuzzyMode::Off).unwrap_err();
        assert_eq!(err.code, ErrorCode::HunkAmbiguous);
    }

    #[test]
    fn not_found() {
        let file = ["a"];
        let h = hunk(vec![
            HunkLine::Delete("missing".into()),
            HunkLine::Add("x".into()),
        ]);
        let err =
            find_unique_match(&file, &h, 0, "f", 0, false, false, FuzzyMode::Off).unwrap_err();
        assert_eq!(err.code, ErrorCode::HunkNotFound);
    }

    #[test]
    fn search_start_skips_earlier_match() {
        let file = ["x", "a", "x", "b"];
        let h = hunk(vec![
            HunkLine::Delete("x".into()),
            HunkLine::Add("X".into()),
        ]);
        let m = find_unique_match(&file, &h, 0, "f", 2, false, false, FuzzyMode::Off).unwrap();
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
        let m = find_unique_match(&file, &h, 0, "f", 0, false, false, FuzzyMode::Off).unwrap();
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
        let err =
            find_unique_match(&file, &h, 0, "f", 0, false, false, FuzzyMode::Off).unwrap_err();
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
        let m = find_unique_match(&file, &h, 0, "f", 0, true, false, FuzzyMode::Off).unwrap();
        assert_eq!(m.start_line, 2);
        assert_eq!(
            m.ins_lines,
            vec!["first".to_string(), "second updated".to_string()]
        );
    }
}
