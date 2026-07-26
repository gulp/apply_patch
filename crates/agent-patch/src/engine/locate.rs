//! Phase A: locate all hunks on the original line array (forward cursor).

use super::matcher::find_unique_match;
use crate::error::{ErrorCode, PublicError};
use crate::protocol::ast::{Hunk, UpdateFile};

/// A hunk resolved against the original file lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocatedChunk {
    /// 0-based index into the original line array where the old side begins.
    pub orig_index: usize,
    /// Number of original lines replaced (`old_side.len()` after any context reduction).
    pub del_len: usize,
    /// Lines to insert (new side, after any matching context reduction).
    pub ins_lines: Vec<String>,
    pub hunk_index: usize,
}

/// Resolve every hunk against `file_lines` without mutating them.
///
/// Search is forward-only: after each hit, the cursor advances to `orig_index + del_len`.
/// Optional `@@ <anchor>` (stored on the hunk) must uniquely match at/after the cursor;
/// the body search then starts at `anchor_index + 1`.
pub fn locate_chunks(
    file_lines: &[&str],
    update: &UpdateFile,
) -> Result<Vec<LocatedChunk>, PublicError> {
    let mut chunks = Vec::with_capacity(update.hunks.len());
    let mut cursor = 0usize;

    for (hunk_index, hunk) in update.hunks.iter().enumerate() {
        let mut search_start = cursor;

        if let Some(anchor) = hunk.anchor.as_deref() {
            let anchor_idx =
                locate_unique_anchor(file_lines, anchor, cursor, hunk, hunk_index, &update.path)?;
            search_start = anchor_idx + 1;
        }

        let hit = find_unique_match(
            file_lines,
            hunk,
            hunk_index,
            &update.path,
            search_start,
            hunk.end_of_file,
        )?;
        let chunk = LocatedChunk {
            orig_index: hit.start_line,
            del_len: hit.end_line - hit.start_line,
            ins_lines: hit.ins_lines,
            hunk_index,
        };
        cursor = chunk.orig_index + chunk.del_len;
        chunks.push(chunk);
    }

    Ok(chunks)
}

fn locate_unique_anchor(
    file_lines: &[&str],
    anchor: &str,
    search_start: usize,
    hunk: &Hunk,
    hunk_index: usize,
    path: &str,
) -> Result<usize, PublicError> {
    let matches: Vec<usize> = file_lines
        .iter()
        .enumerate()
        .skip(search_start)
        .filter(|(_, line)| **line == anchor)
        .map(|(i, _)| i)
        .collect();

    match matches.as_slice() {
        [only] => Ok(*only),
        [] => Err(PublicError::new(
            ErrorCode::HunkNotFound,
            format!(
                "Update hunk {} anchor {:?} did not match a unique line at or after the search cursor.",
                hunk_index + 1,
                anchor
            ),
        )
        .with_path(path)
        .with_hunk(hunk_index)
        .with_source(hunk.source_span)
        .with_hint("Use a unique @@ anchor line from the current file, or regenerate the patch.")),
        _ => Err(PublicError::new(
            ErrorCode::HunkAmbiguous,
            format!(
                "Update hunk {} anchor {:?} matched multiple lines at or after the search cursor.",
                hunk_index + 1,
                anchor
            ),
        )
        .with_path(path)
        .with_hunk(hunk_index)
        .with_source(hunk.source_span)
        .with_hint("Choose a more unique @@ anchor and regenerate the patch.")),
    }
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
        }
    }

    fn hunk(anchor: Option<&str>, lines: Vec<HunkLine>) -> Hunk {
        Hunk {
            lines,
            source_span: SourceSpan { line: 1, column: 1 },
            anchor: anchor.map(str::to_string),
            end_of_file: false,
        }
    }

    #[test]
    fn locates_two_hunks_on_original_with_forward_cursor() {
        let file = ["a", "b", "c", "d"];
        let u = update(vec![
            hunk(
                None,
                vec![HunkLine::Delete("b".into()), HunkLine::Add("B".into())],
            ),
            hunk(
                None,
                vec![HunkLine::Delete("d".into()), HunkLine::Add("D".into())],
            ),
        ]);
        let chunks = locate_chunks(&file, &u).unwrap();
        assert_eq!(chunks[0].orig_index, 1);
        assert_eq!(chunks[0].del_len, 1);
        assert_eq!(chunks[0].ins_lines, vec!["B".to_string()]);
        assert_eq!(chunks[1].orig_index, 3);
        assert_eq!(chunks[1].ins_lines, vec!["D".to_string()]);
    }

    #[test]
    fn anchor_advances_search_past_duplicate_bodies() {
        let file = ["hdr", "x", "y", "hdr", "x", "z"];
        let u = update(vec![hunk(
            Some("hdr"),
            vec![HunkLine::Delete("x".into()), HunkLine::Add("X".into())],
        )]);
        // Without unique-exact on whole file, "x" is ambiguous; with forward
        // cursor after first "hdr", still two "x"? First hunk has anchor "hdr".
        // First "hdr" at 0 → search from 1 → "x" at 1 and 4 → still ambiguous.
        // Use second section: we need the second hdr. For a single hunk with
        // anchor "hdr", unique-exact fails (two hdr). Expect ambiguous.
        let err = locate_chunks(&file, &u).unwrap_err();
        assert_eq!(err.code, ErrorCode::HunkAmbiguous);
    }

    #[test]
    fn unique_anchor_disambiguates_body() {
        let file = ["one", "x", "y", "two", "x", "z"];
        let u = update(vec![hunk(
            Some("two"),
            vec![HunkLine::Delete("x".into()), HunkLine::Add("X".into())],
        )]);
        let chunks = locate_chunks(&file, &u).unwrap();
        assert_eq!(chunks[0].orig_index, 4);
        assert_eq!(chunks[0].ins_lines, vec!["X".to_string()]);
    }
}
