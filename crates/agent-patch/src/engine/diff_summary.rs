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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_replace() {
        let c = diff_line_counts("a\nb\n", "a\nc\n");
        assert_eq!(c.lines_deleted, 1);
        assert_eq!(c.lines_added, 1);
    }
}
