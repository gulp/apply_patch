//! CLI argument parsing.

use crate::app::{default_root, AppConfig};
use crate::error::{ErrorCode, Limits, PublicError};
use crate::match_opts::{FuzzyMode, MatchOptions, RiskMode};
use crate::shadow::ShadowMode;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "agent-patch",
    version,
    about = "Apply localized, transactional patches for coding agents"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Validate without writing files
    #[arg(long)]
    pub check: bool,

    /// Emit immutable execution plan + diffs; no writes
    #[arg(long)]
    pub plan: bool,

    /// Materialize shadow, run command after `--`, promote on success
    #[arg(long)]
    pub verify: bool,

    /// Explicit shell escape for verify (runs via `/bin/sh -c`; prefer `--verify --`)
    #[arg(long, value_name = "SCRIPT")]
    pub verify_shell: Option<String>,

    /// Verify wall-clock timeout (seconds, or Ns/Nm/Nh; default 600s)
    #[arg(long, default_value = "600", value_name = "DURATION")]
    pub verify_timeout: String,

    /// Max bytes captured per verify stdout/stderr stream (default 8 MiB)
    #[arg(long, default_value = "8388608", value_name = "BYTES")]
    pub verify_output_limit: String,

    /// Shadow policy: tree (default, representative) or touched
    #[arg(long, default_value = "tree")]
    pub shadow_mode: String,

    /// Include cache/build dirs in tree shadow (still budgeted)
    #[arg(long)]
    pub shadow_include_caches: bool,

    /// Unique-only fuzzy ladder: off|rstrip|strip
    #[arg(long, default_value = "off")]
    pub fuzzy: String,

    /// Match risk gate: off|warn|refuse
    #[arg(long, default_value = "off")]
    pub risk: String,

    /// Treat full after-state as success when already applied
    #[arg(long)]
    pub idempotent: bool,

    /// Export apply receipt JSON to this path
    #[arg(long)]
    pub receipt: Option<PathBuf>,

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

    /// Verify program and args (place after `--`)
    #[arg(last = true, allow_hyphen_values = true)]
    pub verify_argv: Vec<String>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Environment + store health (no mutation)
    Doctor {
        /// Emit JSON
        #[arg(long)]
        json: bool,
        /// Repository root (default: current directory)
        #[arg(long)]
        root: Option<PathBuf>,
    },
    /// Report lock / journal / object-store health (no mutation)
    Status {
        /// Emit JSON
        #[arg(long)]
        json: bool,
        /// Repository root (default: current directory)
        #[arg(long)]
        root: Option<PathBuf>,
    },
    /// Resolve crash-interrupted transaction journal(s)
    Recover {
        /// Specific transaction id (default: all incomplete)
        #[arg(long)]
        transaction: Option<String>,
        /// Emit JSON
        #[arg(long)]
        json: bool,
        /// Repository root (default: current directory)
        #[arg(long)]
        root: Option<PathBuf>,
    },
    /// Revert a prior apply from a receipt
    Revert {
        /// Receipt JSON path
        receipt: PathBuf,
        /// Emit JSON
        #[arg(long)]
        json: bool,
        /// Repository root (default: current directory)
        #[arg(long)]
        root: Option<PathBuf>,
    },
    /// Garbage-collect unreferenced before-image objects
    Gc {
        /// Report only; do not delete
        #[arg(long)]
        dry_run: bool,
        /// Emit JSON
        #[arg(long)]
        json: bool,
        /// Repository root (default: current directory)
        #[arg(long)]
        root: Option<PathBuf>,
    },
}

impl Cli {
    pub fn into_config(self) -> Result<AppConfig, PublicError> {
        if self.command.is_some() {
            return Err(PublicError::new(
                ErrorCode::InternalError,
                "into_config called with subcommand; dispatch in main.",
            ));
        }
        let verify_mode = self.verify || self.verify_shell.is_some();
        let modes = [self.check, self.plan, verify_mode]
            .into_iter()
            .filter(|x| *x)
            .count();
        if modes > 1 {
            return Err(PublicError::new(
                ErrorCode::InputError,
                "--check, --plan, and --verify/--verify-shell are mutually exclusive.",
            ));
        }
        if self.receipt.is_some() && (self.check || self.plan) {
            return Err(PublicError::new(
                ErrorCode::InputError,
                "--receipt requires a mutating apply.",
            ));
        }
        if self.verify_shell.is_some() && !self.verify_argv.is_empty() {
            return Err(PublicError::new(
                ErrorCode::InputError,
                "--verify-shell cannot combine with argv after `--`; choose one verify form.",
            ));
        }
        if self.verify && self.verify_shell.is_none() && self.verify_argv.is_empty() {
            return Err(PublicError::new(
                ErrorCode::InputError,
                "--verify requires a command after `--` (e.g. agent-patch --verify -- true).",
            ));
        }
        if !verify_mode && !self.verify_argv.is_empty() {
            return Err(PublicError::new(
                ErrorCode::InputError,
                "Trailing argv after `--` is only valid with --verify.",
            ));
        }
        let shadow_mode = ShadowMode::parse(&self.shadow_mode)?;
        let match_opts = MatchOptions {
            fuzzy: FuzzyMode::parse(&self.fuzzy)?,
            risk: RiskMode::parse(&self.risk)?,
        };
        let verify_timeout = crate::verify::parse_verify_timeout(&self.verify_timeout)?;
        let verify_output_limit =
            crate::verify::parse_verify_output_limit(&self.verify_output_limit)?;
        Ok(AppConfig {
            root: self.root.unwrap_or_else(default_root),
            patch_file: self.patch_file,
            check: self.check,
            plan: self.plan,
            verify: verify_mode,
            verify_argv: self.verify_argv,
            verify_shell: self.verify_shell,
            verify_timeout,
            verify_output_limit,
            shadow_mode,
            shadow_include_caches: self.shadow_include_caches,
            match_opts,
            idempotent: self.idempotent,
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
            receipt: self.receipt,
        })
    }
}
