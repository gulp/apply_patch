# agent-patch examples

All commands run from the repository root. Binary is **not** on `PATH`.

## Single-file update

```bash
scripts/agent-patch <<'PATCH'
*** Begin Patch
*** Update File: crates/agent-patch/src/lib.rs
@@
 pub mod app;
-pub mod cli;
+pub mod cli;
+pub mod commit;
*** End Patch
PATCH
```

## Check then apply

```bash
scripts/agent-patch --check < /tmp/change.patch
scripts/agent-patch < /tmp/change.patch
```

## Multi-file atomic patch

```bash
scripts/agent-patch <<'PATCH'
*** Begin Patch
*** Update File: src/a.rs
@@
-old_a
+new_a
*** Add File: src/b.rs
+pub fn b() {}
*** Delete File: src/obsolete.rs
*** End Patch
PATCH
```

## `@@` anchor

```bash
scripts/agent-patch <<'PATCH'
*** Begin Patch
*** Update File: src/lib.rs
@@ fn compute
-    a + b
+    a.saturating_add(b)
*** End Patch
PATCH
```

## `*** End of File` (EOF-prefer)

```bash
scripts/agent-patch <<'PATCH'
*** Begin Patch
*** Update File: src/tail.rs
@@
 trailing context
-old
+new
*** End of File
*** End Patch
PATCH
```

## JSON diagnostics

```bash
scripts/agent-patch --json --check <<'PATCH'
*** Begin Patch
*** Update File: missing.rs
@@
-x
+y
*** End Patch
PATCH
```

## Plan (no writes)

```bash
scripts/agent-patch --plan --json < /tmp/change.patch
```

## Verify-gated apply

```bash
scripts/agent-patch --verify -- cargo test -q < /tmp/change.patch
```

## Verify via shell escape

```bash
scripts/agent-patch --verify-shell 'test -f src/lib.rs' < /tmp/change.patch
```

## Receipt export and revert

```bash
scripts/agent-patch --receipt /tmp/r.json < /tmp/change.patch
scripts/agent-patch revert /tmp/r.json
```

## Status / recover / doctor

```bash
scripts/agent-patch status --json
scripts/agent-patch doctor --json
scripts/agent-patch recover --json
```

On `HUNK_*` / stale failures: read the current region, regenerate from current content, retry — never whole-file overwrite.

On `RECOVERY_REQUIRED`: `scripts/agent-patch recover` — do not delete journals or the lock by hand.
