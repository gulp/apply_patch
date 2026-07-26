pub mod apply;
pub mod diff_summary;
pub mod matcher;

pub use apply::{
    apply_update, detect_newline_style, has_final_newline, join_lines, split_content_lines,
    AppliedText,
};
pub use diff_summary::{diff_line_counts, DiffCounts};
