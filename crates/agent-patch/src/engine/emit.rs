//! Phase B: forward-cursor emit from located chunks.

use super::locate::LocatedChunk;
use crate::error::{ErrorCode, PublicError};

/// Build the updated line list from original lines and ascending located chunks.
pub fn emit_chunks(
    file_lines: &[&str],
    chunks: &[LocatedChunk],
    path: &str,
) -> Result<Vec<String>, PublicError> {
    let mut out: Vec<String> = Vec::with_capacity(file_lines.len());
    let mut cursor = 0usize;

    for chunk in chunks {
        if cursor > chunk.orig_index {
            return Err(PublicError::new(
                ErrorCode::HunkOverlap,
                format!(
                    "Update hunk {} overlaps a previously applied hunk.",
                    chunk.hunk_index + 1
                ),
            )
            .with_path(path)
            .with_hunk(chunk.hunk_index));
        }
        out.extend(
            file_lines[cursor..chunk.orig_index]
                .iter()
                .map(|s| (*s).to_string()),
        );
        out.extend(chunk.ins_lines.iter().cloned());
        cursor = chunk.orig_index + chunk.del_len;
        if cursor > file_lines.len() {
            return Err(PublicError::new(
                ErrorCode::InternalError,
                format!(
                    "Emit cursor {} exceeds file length {} for hunk {}.",
                    cursor,
                    file_lines.len(),
                    chunk.hunk_index + 1
                ),
            )
            .with_path(path)
            .with_hunk(chunk.hunk_index));
        }
    }

    out.extend(file_lines[cursor..].iter().map(|s| (*s).to_string()));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::locate::LocatedChunk;

    #[test]
    fn emits_replacement_and_tail() {
        let file = ["a", "b", "c"];
        let chunks = [LocatedChunk {
            orig_index: 1,
            del_len: 1,
            ins_lines: vec!["B".into()],
            hunk_index: 0,
        }];
        let out = emit_chunks(&file, &chunks, "f").unwrap();
        assert_eq!(out, vec!["a", "B", "c"]);
    }

    #[test]
    fn detects_overlap() {
        let file = ["a", "b", "c"];
        let chunks = [
            LocatedChunk {
                orig_index: 1,
                del_len: 1,
                ins_lines: vec!["B".into()],
                hunk_index: 0,
            },
            LocatedChunk {
                orig_index: 1,
                del_len: 1,
                ins_lines: vec!["Z".into()],
                hunk_index: 1,
            },
        ];
        let err = emit_chunks(&file, &chunks, "f").unwrap_err();
        assert_eq!(err.code, ErrorCode::HunkOverlap);
    }
}
