//! agent-patch library.

#![allow(clippy::result_large_err)]

pub mod app;
pub mod cli;
pub mod commit;
pub mod diagnostics;
pub mod doctor;
pub mod engine;
pub mod error;
pub mod events;
pub mod failpoints;
pub mod fs;
pub mod gc;
pub mod input;
pub mod journal;
pub mod limits;
pub mod match_opts;
pub mod objects;
pub mod oracle;
pub mod path_policy;
pub mod plan;
pub mod protocol;
pub mod receipt;
pub mod recover;
pub mod revert;
pub mod root_lock;
pub mod shadow;
pub mod snapshot;
pub mod status;
pub mod store_layout;
pub mod telemetry;
pub mod validate;
pub mod verify;
