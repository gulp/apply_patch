//! CLI argument parsing.

use crate::app::{default_root, AppConfig};
use crate::error::Limits;
use clap::Parser;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "agent-patch",
    version,
    about = "Apply localized, transactional patches for coding agents"
)]
pub struct Cli {
    /// Validate without writing files
    #[arg(long)]
    pub check: bool,

    /// Repository root (default: current directory)
    #[arg(long)]
    pub root: Option<PathBuf>,

    /// Emit a single JSON object on stdout
    #[arg(long)]
    pub json: bool,

    /// Suppress success summary on stdout
    #[arg(long)]
    pub quiet: bool,

    /// Maximum number of files in one patch
    #[arg(long, default_value_t = Limits::default().max_files)]
    pub max_files: usize,

    /// Maximum patch size in bytes
    #[arg(long, default_value_t = Limits::default().max_patch_bytes)]
    pub max_patch_bytes: usize,

    /// Maximum size of any single target file in bytes
    #[arg(long, default_value_t = Limits::default().max_file_bytes)]
    pub max_file_bytes: usize,

    /// Patch file path; if omitted, read from stdin
    pub patch_file: Option<PathBuf>,
}

impl Cli {
    pub fn into_config(self) -> AppConfig {
        AppConfig {
            root: self.root.unwrap_or_else(default_root),
            patch_file: self.patch_file,
            check: self.check,
            json: self.json,
            quiet: self.quiet,
            limits: Limits {
                max_patch_bytes: self.max_patch_bytes,
                max_file_bytes: self.max_file_bytes,
                max_files: self.max_files,
                max_hunks_per_file: Limits::default().max_hunks_per_file,
                max_total_hunks: Limits::default().max_total_hunks,
            },
            fsync: true,
        }
    }
}
