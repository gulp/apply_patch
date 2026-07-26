---
name: agent-patch
description: >-
  Apply localized multi-hunk and multi-file edits with the repo-local agent-patch
  CLI (V4A patches, transactional, fail-closed). Use when editing this repository
  with more than one hunk or file, when the user supplies a patch, when dogfooding
  agent-patch, or when AGENTS.md / scripts/agent-patch is relevant. Prefer over
  whole-file rewrites and over stacking many single-string replacements.
---

# agent-patch

Repo-local V4A patch CLI for fail-closed, transactional edits.

## Invocation (not on PATH)

`agent-patch` is **not** on `PATH` after a bare clone. Prefer the repo wrapper (canonical):

```bash
scripts/agent-patch [OPTIONS] [PATCH_FILE]
```

Resolution order inside the wrapper:

1. `target/release/agent-patch` if executable
2. else `target/debug/agent-patch` if executable
3. else `cargo run --quiet --bin agent-patch -- …`

Do not assume `agent-patch` works as a bare command. Do not guess flags — run `scripts/agent-patch --help`.

Optional bare command in-repo: `export PATH="$PWD/scripts:$PATH"` or `cp .envrc.example .envrc && direnv allow`. Global: `cargo install --path crates/agent-patch --force`. See README.

Useful options: `--check` (validate, no writes), `--json`, `--quiet`, `--root <PATH>`.

## When to dogfood (real conversation)

In this repo, prefer `scripts/agent-patch` for:

| Use patch | Use native exact-replace / small edit tool |
| --- | --- |
| Multiple hunks in one file | One small unique string change |
| Multiple files in one atomic change | Trivial typo / single identifier |
| Add or delete files with context | |
| User-supplied V4A patch text | |
| EOF-prefer or `@@` anchor updates | |

**Dogfood rule:** if the change would be clearer as a contextual hunk (or several), apply it with `scripts/agent-patch` instead of rewriting the whole file or issuing a long chain of one-off replacements.

## Apply workflow

1. Read the current affected region(s) so old-side lines match exactly.
2. Build a V4A patch (stdin heredoc or a patch file). Prefer stdin heredoc to the process — **do not** nest `<<'PATCH'` inside `$(...)`.
3. Optional: `scripts/agent-patch --check <<'PATCH' …` before writing.
4. Apply: `scripts/agent-patch <<'PATCH' …`
5. On failure: read the error (`HUNK_NOT_FOUND`, `HUNK_AMBIGUOUS`, …), re-read the file region, regenerate the patch from **current** content, retry. Never recover by overwriting the entire file.

```bash
scripts/agent-patch <<'PATCH'
*** Begin Patch
*** Update File: path/to/file.rs
@@
 context line
-old line
+new line
*** End Patch
PATCH
```

Multi-file / add / delete / EOF examples: see [examples.md](examples.md). Protocol detail: [docs/protocol.md](../../../docs/protocol.md).

## Constraints

- Unique exact match only — ambiguous context fails closed.
- `*** Move File` / `*** Move to:` unsupported.
- Update preserves file LF/CRLF; mixed endings rejected.
- Paths are repo-relative under `--root` (default: cwd).

## After apply

Run the checks the change warrants (`cargo test`, `scripts/dogfood`, etc.). For accepting the tool itself, `scripts/dogfood` is the end-to-end gate.
