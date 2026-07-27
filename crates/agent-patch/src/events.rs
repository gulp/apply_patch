//! Optional JSONL event log (observability adapter; not required for correctness).
//!
//! Enable with `AGENT_PATCH_EVENT_LOG=1` (writes under `.agent-patch/events/`)
//! or `AGENT_PATCH_EVENT_LOG=/path/to/file.jsonl`.

use crate::store_layout::{agent_patch_dir, ensure_layout};
use serde::Serialize;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize)]
pub struct EventRecord<'a> {
    pub version: u32,
    pub ts: String,
    pub phase: &'a str,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_digest: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<&'a str>,
}

fn resolve_log_path(root: &Path) -> Option<PathBuf> {
    let raw = std::env::var("AGENT_PATCH_EVENT_LOG").ok()?;
    if raw.is_empty() || raw == "0" || raw.eq_ignore_ascii_case("false") {
        return None;
    }
    if raw == "1" || raw.eq_ignore_ascii_case("true") {
        let _ = ensure_layout(root);
        let dir = agent_patch_dir(root).join("events");
        let _ = std::fs::create_dir_all(&dir);
        return Some(dir.join("events.jsonl"));
    }
    Some(PathBuf::from(raw))
}

/// Append one metadata event. Failures are ignored (adapter never changes exit semantics).
pub fn emit(root: &Path, record: &EventRecord<'_>) {
    let Some(path) = resolve_log_path(root) else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Ok(mut line) = serde_json::to_string(record) else {
        return;
    };
    line.push('\n');
    let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) else {
        return;
    };
    let _ = f.write_all(line.as_bytes());
}

pub fn now_ts() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".into())
}

/// Remove orphaned verify shadow directories left by crashed processes.
pub fn cleanup_stale_shadows(root: &Path) -> usize {
    let dir = agent_patch_dir(root).join("shadows");
    if !dir.is_dir() {
        return 0;
    }
    let mut removed = 0;
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return 0;
    };
    for ent in entries.flatten() {
        let path = ent.path();
        if path.is_dir() && std::fs::remove_dir_all(&path).is_ok() {
            removed += 1;
        }
    }
    removed
}
