//! Human and JSON diagnostics.

use crate::argv_normalize::CoachNote;
use crate::error::PublicError;
use crate::oracle::CandidateSpan;
use crate::plan::ExecutionPlanJson;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct JsonSuccess {
    pub version: u32,
    pub ok: bool,
    pub mode: String,
    pub root: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction_id: Option<String>,
    pub summary: JsonSummary,
    pub files: Vec<JsonFileResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<ExecutionPlanJson>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verify: Option<crate::verify::VerifyReport>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub already_applied: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coach: Option<CoachNote>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coach: Option<CoachNote>,
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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<CandidateSpan>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repair_patch: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggestions: Vec<String>,
    pub root_changed: bool,
    pub recovery_required: bool,
}

#[derive(Debug, Serialize)]
pub struct JsonSource {
    pub line: usize,
    pub column: usize,
}

pub fn emit_error_json(err: &PublicError) -> String {
    emit_error_json_with_coach(err, None)
}

pub fn emit_error_json_with_coach(err: &PublicError, coach: Option<&CoachNote>) -> String {
    let version = if err.candidates.is_empty()
        && err.repair_patch.is_none()
        && err.examples.is_empty()
        && err.suggestions.is_empty()
        && coach.is_none()
    {
        1
    } else {
        2
    };
    let body = JsonErrorEnvelope {
        version,
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
            candidates: err.candidates.clone(),
            repair_patch: err.repair_patch.clone(),
            examples: err.examples.clone(),
            suggestions: err.suggestions.clone(),
            root_changed: err.root_changed,
            recovery_required: err.recovery_required,
        },
        coach: coach.cloned(),
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
    if !err.candidates.is_empty() {
        out.push_str(&format!("\n  candidates: {}", err.candidates.len()));
        for (i, c) in err.candidates.iter().take(3).enumerate() {
            out.push_str(&format!(
                "\n  candidate[{i}] lines {}..{}:\n{}",
                c.start_line, c.end_line, c.excerpt
            ));
        }
    }
    if let Some(repair) = &err.repair_patch {
        out.push_str("\n  repair_patch:\n");
        out.push_str(repair);
    }
    if let Some(hint) = &err.hint {
        out.push_str(&format!("\n  hint: {hint}"));
    }
    for ex in &err.examples {
        out.push_str(&format!("\n  example: {ex}"));
    }
    for s in &err.suggestions {
        out.push_str(&format!("\n  suggestion: {s}"));
    }
    out
}

pub fn emit_success_human(summary: &JsonSummary, mode: &str, warnings: &[String]) -> String {
    let mut out = format!(
        "{mode} ok: {} file(s) ({} added, {} updated, {} deleted); +{} -{} lines; {} hunk(s); {} ms",
        summary.files_total,
        summary.files_added,
        summary.files_updated,
        summary.files_deleted,
        summary.lines_added,
        summary.lines_deleted,
        summary.hunks_applied,
        summary.duration_ms
    );
    for w in warnings {
        out.push_str(&format!("\n  warning: {w}"));
    }
    out
}

/// Merge optional coach into an already-serialized JSON object (subcommand reports).
pub fn json_with_coach<T: Serialize>(value: &T, coach: Option<&CoachNote>) -> String {
    let mut v = serde_json::to_value(value).unwrap_or(serde_json::json!({"ok": false}));
    if let Some(c) = coach {
        if let Some(obj) = v.as_object_mut() {
            obj.insert("coach".into(), serde_json::to_value(c).unwrap_or_default());
            obj.insert("version".into(), serde_json::json!(2));
        }
    }
    serde_json::to_string_pretty(&v).unwrap_or_else(|_| {
        r#"{"ok":false,"error":"serialize"}"#.to_string()
    })
}
