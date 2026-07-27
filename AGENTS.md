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
