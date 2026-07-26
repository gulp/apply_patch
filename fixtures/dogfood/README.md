# Dogfood fixtures

Isolated sample tree for exercising `scripts/agent-patch` without touching crate sources.

```bash
agent-patch --root fixtures/dogfood <<'P'
*** Begin Patch
*** Update File: docs/note.md
@@
-Timeout is 60 seconds.
+Timeout is 90 seconds.
*** End Patch
P
```

With direnv (`.envrc` → `PATH_add scripts`), the bare `agent-patch` command works; otherwise use `scripts/agent-patch`. Always pass `--root fixtures/dogfood` so paths stay inside this folder.
