//! Diff summary using `similar`.

use similar::{ChangeTag, TextDiff};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiffCounts {
    pub lines_added: usize,
    pub lines_deleted: usize,
}

pub fn diff_line_counts(before: &str, after: &str) -> DiffCounts {
    let diff = TextDiff::from_lines(before, after);
    let mut lines_added = 0usize;
    let mut lines_deleted = 0usize;
    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Insert => lines_added += 1,
            ChangeTag::Delete => lines_deleted += 1,
            ChangeTag::Equal => {}
        }
    }
    DiffCounts {
        lines_added,
        lines_deleted,
    }
}

/// Observational unified diff for `--plan` output (not an apply backend).
pub fn unified_diff(path: &str, before: &str, after: &str) -> String {
    let diff = TextDiff::from_lines(before, after);
    format!(
        "{}",
        diff.unified_diff()
            .context_radius(3)
            .header(&format!("a/{path}"), &format!("b/{path}"))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_replace() {
        let c = diff_line_counts("a\nb\n", "a\nc\n");
        assert_eq!(c.lines_deleted, 1);
        assert_eq!(c.lines_added, 1);
    }

    #[test]
    fn unified_mentions_path() {
        let u = unified_diff("x.txt", "a\n", "b\n");
        assert!(u.contains("a/x.txt") || u.contains("---"));
    }
}
