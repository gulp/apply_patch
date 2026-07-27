//! Oracle candidates and match evidence (contract-v2 caps).

use serde::Serialize;

pub const MAX_CANDIDATES: usize = 8;
pub const MAX_EXCERPT_LINES: usize = 20;
pub const MAX_CANDIDATE_PAYLOAD_BYTES: usize = 64 * 1024;
pub const MAX_REPAIR_PATCH_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MatchLevel {
    Exact,
    ContextReduced,
    Rstrip,
    Strip,
    Eof,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MatchEvidence {
    pub path: String,
    pub hunk_index: usize,
    pub accepted_level: MatchLevel,
    pub candidate_count: usize,
    pub retained_context_lead: usize,
    pub retained_context_trail: usize,
    pub used_anchor: bool,
    pub used_eof: bool,
    pub nearby_twins: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CandidateSpan {
    /// 1-based start line in the file (inclusive).
    pub start_line: usize,
    /// 0-based exclusive end index into the line array (same as match end).
    pub end_line: usize,
    pub excerpt: String,
}

pub fn build_candidates(file_lines: &[&str], ranges: &[(usize, usize)]) -> Vec<CandidateSpan> {
    let mut out = Vec::new();
    let mut payload = 0usize;
    for &(start, end) in ranges.iter().take(MAX_CANDIDATES) {
        let excerpt_end = end.min(start + MAX_EXCERPT_LINES).min(file_lines.len());
        let excerpt_start = start.min(file_lines.len());
        if excerpt_start >= excerpt_end && excerpt_start < file_lines.len() {
            // empty span — still report location
        }
        let excerpt = if excerpt_start < excerpt_end {
            file_lines[excerpt_start..excerpt_end]
                .iter()
                .enumerate()
                .map(|(i, line)| format!("{}: {line}", excerpt_start + i + 1))
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            String::new()
        };
        let add = excerpt.len().saturating_add(32);
        if payload + add > MAX_CANDIDATE_PAYLOAD_BYTES && !out.is_empty() {
            break;
        }
        payload += add;
        out.push(CandidateSpan {
            start_line: start + 1,
            end_line: end,
            excerpt,
        });
    }
    out
}

pub fn truncate_repair_patch(patch: String) -> String {
    if patch.len() <= MAX_REPAIR_PATCH_BYTES {
        patch
    } else {
        let mut s = patch[..MAX_REPAIR_PATCH_BYTES].to_string();
        s.push_str("\n…");
        s
    }
}

/// Draft a repair Update targeting the first candidate span.
///
/// Replaces the candidate's current lines with the hunk's intended new side,
/// retaining up to two surrounding context lines from the file. Returns `None`
/// when there is no usable candidate span.
pub fn draft_repair_patch(
    path: &str,
    file_lines: &[&str],
    new_side: &[&str],
    ranges: &[(usize, usize)],
) -> Option<String> {
    let &(start, end) = ranges.first()?;
    if start > file_lines.len() {
        return None;
    }
    let end = end.min(file_lines.len());
    if start > end {
        return None;
    }
    let ctx_before = start.saturating_sub(2);
    let ctx_after = (end + 2).min(file_lines.len());

    let mut body = String::from("*** Begin Patch\n");
    body.push_str("*** Update File: ");
    body.push_str(path);
    body.push('\n');
    body.push_str("@@\n");
    for line in &file_lines[ctx_before..start] {
        body.push(' ');
        body.push_str(line);
        body.push('\n');
    }
    for line in &file_lines[start..end] {
        body.push('-');
        body.push_str(line);
        body.push('\n');
    }
    for line in new_side {
        body.push('+');
        body.push_str(line);
        body.push('\n');
    }
    for line in &file_lines[end..ctx_after] {
        body.push(' ');
        body.push_str(line);
        body.push('\n');
    }
    body.push_str("*** End Patch\n");
    Some(truncate_repair_patch(body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draft_repair_targets_first_candidate() {
        let file = ["aa", "bb", "cc", "dd"];
        let patch = draft_repair_patch("f.txt", &file, &["XX"], &[(1, 2)]).unwrap();
        assert!(patch.contains("*** Update File: f.txt"));
        assert!(patch.contains("-bb\n"));
        assert!(patch.contains("+XX\n"));
        assert!(patch.contains(" aa\n") || patch.contains(" aa"));
    }

    #[test]
    fn truncate_marks_oversized() {
        let big = "x".repeat(MAX_REPAIR_PATCH_BYTES + 10);
        let out = truncate_repair_patch(big);
        assert!(out.ends_with('…'));
        assert!(out.len() <= MAX_REPAIR_PATCH_BYTES + 4);
    }
}
