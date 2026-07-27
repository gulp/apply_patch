# Agent instructions

Operational guide for coding agents in this repository. Canonical copy: [AGENTS.md](AGENTS.md).

## Localized file editing

Use the native exact-replacement edit operation for one small, unique replacement.

Use `scripts/agent-patch` when:

- a change contains multiple related hunks;
- several files should change atomically;
- additions or removals are clearer as contextual hunks;
- the user supplied a patch;
- exact replacement would require copying a large unchanged block;
- verify-gated apply, receipts/revert, or recovery are required.

Invoke with a single-quoted heredoc:

```bash
scripts/agent-patch <<'PATCH'
*** Begin Patch
*** Update File: path/to/file
@@
-old text
+new text
*** End Patch
PATCH
```

For a nontrivial patch, validate first:

```bash
scripts/agent-patch --check < /tmp/change.patch
scripts/agent-patch --plan --json < /tmp/change.patch
```

When a patch fails, read the current affected region and regenerate the patch.
Do not recover by overwriting the entire existing file.

On `RECOVERY_REQUIRED`, run `scripts/agent-patch recover` — do not delete journals or the lock by hand.

Do not guess unsupported flags. Run `scripts/agent-patch --help`.

Contract: [docs/contract-v1.md](docs/contract-v1.md), [docs/contract-v2.md](docs/contract-v2.md).  
Protocol: [docs/protocol.md](docs/protocol.md).  
Errors: [docs/errors.md](docs/errors.md).

## Hard-won rules (keep short)

**Facts**
- Canonical CLI: `scripts/agent-patch` (release → debug → `cargo run`). Rebuild release after engine changes; `doctor` checks freshness.
- Store: `<root>/.agent-patch/{lock,objects,transactions,receipts,shadows,events}`. Guarantee is recoverability to all-before/all-after — not multi-file atomic visibility.
- Unique-exact default; `--fuzzy` unique-only (never first-match). `Move` / `translate` backlog. Docs: present tense only.
- Verify: `--verify -- <PROG>…` or `--verify-shell`. Patch path **before** `--`. Subcommand flags on the subcommand (`revert --json --root …`), not top-level.
- Clap: verify argv is `last = true` only — never combine with `trailing_var_arg` (exit 101 panic).
- `recover`: mixed before/after across files → restore **all** before. Dead-PID lock reclaim OK; **never** delete journals/lock by hand or via stale-lock heuristics.
- Leave `.envrc` untracked. Optional: `AGENT_PATCH_EVENT_LOG=1`. Crash matrix: `cargo test --features failpoints --test crash_matrix`. Gate: `scripts/dogfood`.

**Do**
- Write durable `PREPARED` journal + CAS before-images before any visible rename/delete; terminal `ROLLED_BACK` after successful in-process rollback (do not leave `ROLLING_BACK`).
- Ignore `.agent-patch/` in fixture tree equality checks.

**Don't**
- Claim multi-file atomic visibility. Bypass argv/shell verify contract. Commit `.envrc`.
