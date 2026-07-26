//! Lightweight local telemetry.

use std::time::Instant;

#[derive(Debug, Clone)]
pub struct InvocationTimers {
    pub start: Instant,
}

impl InvocationTimers {
    pub fn start() -> Self {
        Self {
            start: Instant::now(),
        }
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }
}

pub fn debug_enabled() -> bool {
    matches!(
        std::env::var("AGENT_PATCH_LOG").as_deref(),
        Ok("debug") | Ok("trace") | Ok("info")
    )
}

pub fn debug_log(phase: &str, detail: &str) {
    if debug_enabled() {
        eprintln!("level=debug phase={phase} {detail}");
    }
}
