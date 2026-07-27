//! Environment / binary / store health checks.

use crate::error::PublicError;
use crate::status::{status, StatusCheck, StatusReport};
use serde::Serialize;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

#[derive(Debug, Serialize)]
pub struct DoctorReport {
    pub root: String,
    pub ok: bool,
    pub selected_binary: String,
    pub checks: Vec<StatusCheck>,
}

pub fn doctor(root: &Path) -> Result<DoctorReport, PublicError> {
    let mut checks = Vec::new();
    let selected = env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "unknown".into());

    // PATH / resolution
    let on_path = Command::new("sh")
        .arg("-c")
        .arg("command -v agent-patch")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    checks.push(StatusCheck {
        name: "path".into(),
        level: "ok".into(),
        message: match on_path {
            Some(p) => format!("agent-patch resolves to {p}"),
            None => "agent-patch not found on PATH (use scripts/agent-patch or cargo run)".into(),
        },
    });

    // Release binary freshness vs crate mtime (best-effort)
    let crate_src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs");
    if let (Ok(exe), Ok(src_meta)) = (env::current_exe(), std::fs::metadata(&crate_src)) {
        if let (Ok(exe_meta), Ok(src_mtime)) = (std::fs::metadata(&exe), src_meta.modified()) {
            if let Ok(exe_mtime) = exe_meta.modified() {
                if exe_mtime < src_mtime {
                    let level = if exe.ends_with("release/agent-patch") {
                        "error"
                    } else {
                        "warn"
                    };
                    checks.push(StatusCheck {
                        name: "freshness".into(),
                        level: level.into(),
                        message: format!(
                            "Selected binary ({}) is older than sources; rebuild release before trusting scripts/agent-patch.",
                            exe.display()
                        ),
                    });
                } else {
                    checks.push(StatusCheck {
                        name: "freshness".into(),
                        level: "ok".into(),
                        message: "Selected binary is at least as new as crate sources.".into(),
                    });
                }
            }
        }
    } else {
        let _ = SystemTime::now();
        checks.push(StatusCheck {
            name: "freshness".into(),
            level: "warn".into(),
            message: "Could not compare binary and source mtimes.".into(),
        });
    }

    // Fold status journal/lock checks
    let status_report: StatusReport = status(root)?;

    let shadow_dir = crate::store_layout::agent_patch_dir(root).join("shadows");
    let orphan_shadows = std::fs::read_dir(&shadow_dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .count()
        })
        .unwrap_or(0);
    checks.push(StatusCheck {
        name: "shadow_retention".into(),
        level: if orphan_shadows == 0 { "ok".into() } else { "warn".into() },
        message: if orphan_shadows == 0 {
            "No leftover verify shadows.".into()
        } else {
            format!(
                "{orphan_shadows} leftover verify shadow(s) under .agent-patch/shadows (removed on recover / next verify lifecycle)."
            )
        },
    });
    checks.extend(status_report.checks);

    let ok = checks.iter().all(|c| c.level != "error");
    Ok(DoctorReport {
        root: root.display().to_string(),
        ok,
        selected_binary: selected,
        checks,
    })
}
