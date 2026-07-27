//! Test-only crash injection. Enabled with `--features failpoints`.
//!
//! Set `AGENT_PATCH_FAILPOINT=<name>` to abort at a named killpoint.
//! Without the feature, `hit` is a no-op.

/// Named killpoints used by the crash matrix.
pub mod names {
    pub const AFTER_PREPARED: &str = "after_prepared";
    pub const BEFORE_VISIBLE_MUTATE: &str = "before_visible_mutate";
    pub const AFTER_FIRST_VISIBLE: &str = "after_first_visible";
    pub const BEFORE_COMPLETED: &str = "before_completed";
}

/// Abort the process when `AGENT_PATCH_FAILPOINT` equals `name` (failpoints feature only).
pub fn hit(name: &str) {
    #[cfg(feature = "failpoints")]
    {
        if std::env::var("AGENT_PATCH_FAILPOINT").ok().as_deref() == Some(name) {
            eprintln!("agent-patch: failpoint abort at {name}");
            std::process::abort();
        }
    }
    let _ = name;
}
