pub mod apply;
pub mod diff_summary;
pub mod emit;
pub mod locate;
pub mod matcher;

pub use apply::{
    apply_update, detect_newline_style, has_final_newline, join_lines, split_content_lines,
    AppliedText,
};
pub use diff_summary::{diff_line_counts, DiffCounts};
pub use emit::emit_chunks;
pub use locate::{locate_chunks, LocatedChunk};
