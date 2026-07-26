//! Human and JSON diagnostics.

use crate::error::PublicError;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct JsonSuccess {
    pub version: u32,
    pub ok: bool,
    pub mode: String,
    pub root: String,
    pub summary: JsonSummary,
    pub files: Vec<JsonFileResult>,
}

#[derive(Debug, Serialize)]
pub struct JsonSummary {
    pub files_total: usize,
    pub files_added: usize,
    pub files_updated: usize,
    pub files_deleted: usize,
    pub hunks_applied: usize,
    pub lines_added: usize,
    pub lines_deleted: usize,
    pub duration_ms: u64,
}

#[derive(Debug, Serialize)]
pub struct JsonFileResult {
    pub path: String,
    pub operation: String,
    pub hunks: usize,
    pub lines_added: usize,
    pub lines_deleted: usize,
    pub before_blake3: Option<String>,
    pub after_blake3: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct JsonErrorEnvelope {
    pub version: u32,
    pub ok: bool,
    pub error: JsonErrorBody,
}

#[derive(Debug, Serialize)]
pub struct JsonErrorBody {
    pub code: String,
    pub exit_code: u8,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hunk_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<JsonSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct JsonSource {
    pub line: usize,
    pub column: usize,
}

pub fn emit_error_json(err: &PublicError) -> String {
    let body = JsonErrorEnvelope {
        version: 1,
        ok: false,
        error: JsonErrorBody {
            code: err.code.as_str().to_string(),
            exit_code: err.exit_code(),
            message: err.message.clone(),
            path: err.path.clone(),
            operation_index: err.operation_index,
            hunk_index: err.hunk_index,
            source: err.source.map(|s| JsonSource {
                line: s.line,
                column: s.column,
            }),
            hint: err.hint.clone(),
        },
    };
    serde_json::to_string_pretty(&body).unwrap_or_else(|_| {
        r#"{"version":1,"ok":false,"error":{"code":"INTERNAL_ERROR","exit_code":6,"message":"JSON serialization failed"}}"#.to_string()
    })
}

pub fn emit_error_human(err: &PublicError) -> String {
    let mut out = format!("error[{}]: {}", err.code.as_str(), err.message);
    if let Some(path) = &err.path {
        out.push_str(&format!("\n  path: {path}"));
    }
    if let Some(op) = err.operation_index {
        out.push_str(&format!("\n  operation_index: {op}"));
    }
    if let Some(h) = err.hunk_index {
        out.push_str(&format!("\n  hunk_index: {h}"));
    }
    if let Some(s) = err.source {
        out.push_str(&format!("\n  source: {}:{}", s.line, s.column));
    }
    if let Some(hint) = &err.hint {
        out.push_str(&format!("\n  hint: {hint}"));
    }
    out
}

pub fn emit_success_human(summary: &JsonSummary, check: bool) -> String {
    let mode = if check { "check" } else { "apply" };
    format!(
        "{mode} ok: {} file(s) ({} added, {} updated, {} deleted); +{} -{} lines; {} hunk(s); {} ms",
        summary.files_total,
        summary.files_added,
        summary.files_updated,
        summary.files_deleted,
        summary.lines_added,
        summary.lines_deleted,
        summary.hunks_applied,
        summary.duration_ms
    )
}
