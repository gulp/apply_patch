//! agent-patch library.

#![allow(clippy::result_large_err)]

pub mod app;
pub mod cli;
pub mod commit;
pub mod diagnostics;
pub mod engine;
pub mod error;
pub mod fs;
pub mod input;
pub mod limits;
pub mod path_policy;
pub mod plan;
pub mod protocol;
pub mod snapshot;
pub mod telemetry;
pub mod validate;
