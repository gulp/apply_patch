# agent-patch

Deterministic, non-interactive CLI for applying localized, transactional file patches. Coding agents use it for safe multi-hunk and multi-file edits without rewriting whole files.

The patch dialect matches the Codex / OpenAI Agents V4A family (`*** Begin Patch` … `*** End Patch`). The runtime is fail-closed: unique exact hunk matching, root confinement, and all-or-nothing commit with rollback.

## Build

```bash
cargo build --release
```

Binary: `target/release/agent-patch`. The compiled name is `agent-patch`, but it is **not** installed on your `PATH` after clone.

### Run (recommended)

Use the repo wrapper — no install, picks release → debug → `cargo run`:

```bash
scripts/agent-patch --help
```

This is the canonical entrypoint for agents and humans. Prefer it in docs and scripts.

### Optional: put `agent-patch` on PATH while in this repo

The wrapper file is already named `scripts/agent-patch`, so prepending `scripts/` to `PATH` makes the bare command work:

```bash
# session-only
export PATH="$PWD/scripts:$PATH"
agent-patch --help
```

Or with [direnv](https://direnv.net/):

```bash
cp .envrc.example .envrc   # PATH_add scripts
direnv allow
agent-patch --help
```

### Optional: install globally (Cargo bin)

Requires `~/.cargo/bin` on your `PATH` (normal for Rust toolchains):

```bash
cargo install --path crates/agent-patch --force
agent-patch --help
```

Use this when you want `agent-patch` outside this repo. Day-to-day work in-tree should still prefer `scripts/agent-patch` so you always run the tree you checked out.

## CLI

```text
agent-patch [OPTIONS] [PATCH_FILE]
```

| Option | Description |
| --- | --- |
| `--check` | Validate and apply in memory; write nothing |
| `--root <PATH>` | Repository root (default: current directory) |
| `--json` | Emit exactly one JSON object on stdout |
| `--quiet` | Suppress the human success summary |
| `--max-files <N>` | Max files per patch (default: 128) |
| `--max-patch-bytes <N>` | Max patch size (default: 4 MiB) |
| `--max-file-bytes <N>` | Max size of any target file (default: 16 MiB) |
| `-h`, `--help` | Help |
| `-V`, `--version` | Version |

If `PATCH_FILE` is omitted, the patch is read from stdin.

### Examples

Apply from stdin:

```bash
scripts/agent-patch <<'PATCH'
*** Begin Patch
*** Update File: src/config.rs
@@
 pub const RETRIES: usize = 2;
-pub const TIMEOUT_SECS: u64 = 30;
+pub const TIMEOUT_SECS: u64 = 45;
*** End Patch
PATCH
```

Validate only:

```bash
scripts/agent-patch --check < change.patch
```

Apply a patch file with JSON result:

```bash
scripts/agent-patch --json /tmp/change.patch
```

Constrain the root:

```bash
scripts/agent-patch --root "$PWD" < change.patch
```

EOF-prefer update (trailing duplicate context):

```bash
scripts/agent-patch <<'PATCH'
*** Begin Patch
*** Update File: src/tail.rs
@@
 context
-old
+new
*** End of File
*** End Patch
PATCH
```

## Protocol (v1)

Envelope:

```text
*** Begin Patch
<operations>
*** End Patch
```

| Operation | Role |
| --- | --- |
| `*** Add File: path` | Create a missing file (`+` content lines) |
| `*** Update File: path` | Contextual hunks (`@@` / `@@ <anchor>`, ` `, `-`, `+`); optional `*** End of File` |
| `*** Delete File: path` | Remove an existing regular file |

`*** Move File` / `*** Move to:` are not supported in v1 (design notes: [docs/design/move.md](docs/design/move.md)).

### Update matching

- Hunks start with bare `@@` or `@@ <anchor>` (unique exact line at/after the search cursor). Unified-diff numeric headers such as `@@ -1,3 +1,4 @@` are ignored as location math; the body still matches exactly.
- Matching is unique and exact on logical lines. Ambiguous or missing context fails closed (`HUNK_AMBIGUOUS` / `HUNK_NOT_FOUND`). No whitespace-fuzzy or first-match-wins behavior.
- Optional trailing `*** End of File` prefers an exact match aligned at EOF, then falls back to unique forward search.
- Controlled edge-context reduction applies only when the reduced needle remains unique.
- Apply resolves all hunks on the original line array, then emits with a forward cursor (no rematch against a mutating buffer).
- On update, the file’s LF or CRLF style wins; mixed endings are rejected. Add File content uses LF.

Paths are repository-relative. Absolute paths, `.` / `..`, and symlink escape outside `--root` are rejected.

Full grammar: [docs/protocol.md](docs/protocol.md). Frozen semantics: [docs/contract-v1.md](docs/contract-v1.md).

## Behavior

1. Parse and validate the entire patch.
2. Snapshot every affected path under the configured root.
3. Apply all updates in memory (`locate_chunks` → `emit_chunks`).
4. On `--check`, report success or failure and exit without writes.
5. Otherwise revalidate content fingerprints (BLAKE3), commit via same-directory temps and atomic rename, and roll back on failure.

`--check` runs the same parse / validate / snapshot / apply path as apply mode; it never creates temps or mutates the tree.

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | Success |
| 1 | Patch does not apply to current content |
| 2 | Malformed or unsupported patch |
| 3 | Filesystem / I/O failure |
| 4 | Unsafe path or policy violation |
| 5 | Concurrent modification detected |
| 6 | Internal invariant violation / rollback failed |
| 7 | Configured resource limit exceeded |

Granular codes (`HUNK_NOT_FOUND`, `INVALID_PATH`, …): [docs/errors.md](docs/errors.md).

## Output

**Human mode (default):** concise success summary on stdout; diagnostics on stderr.

**JSON mode (`--json`):** one JSON object on stdout (`ok: true` with summary/files, or `ok: false` with `error`). No ANSI color. Content fingerprints use BLAKE3 (`blake3` in JSON fields).

## Development

```bash
scripts/test      # cargo fmt --check, clippy -D warnings, cargo test
scripts/lint      # fmt --check + clippy
scripts/dogfood   # end-to-end gate (multi-hunk/file, stale, ambiguous, add/delete, path safety, --check, EOF)
scripts/bench     # Criterion: cargo bench --workspace
```

Tests of note:

| Suite | What it covers |
| --- | --- |
| `cargo test --workspace` | Unit + CLI integration (atomicity, concurrency, path safety, limits) |
| `tests/codex_scenarios.rs` | Portable Codex fixture subset under `tests/fixtures/codex-scenarios/` |
| `scripts/dogfood` | Phase-8 style acceptance; rebuilds release before running |
| `fuzz/` | `cargo-fuzz` targets: `parse_patch`, `path_policy`, `apply_update` (see [fuzz/README.md](fuzz/README.md)) |
| `benches/apply_update.rs` | Locate→emit throughput on a multi-thousand-line buffer |

CI (Linux and macOS): `fmt` + `clippy -D warnings` + `cargo test --workspace` + `scripts/dogfood`.

Workspace layout:

```text
crates/agent-patch/   # library + binary (+ benches, integration tests)
fuzz/                 # cargo-fuzz workspace (parse / path / apply)
scripts/              # agent-patch, test, lint, dogfood, bench
docs/                 # protocol, contract, errors, design, research
```

Design: [docs/design/](docs/design/). Threat model: [docs/threat-model.md](docs/threat-model.md). Active plan: [IMPLEMENTATION_PLAN.md](IMPLEMENTATION_PLAN.md). Backlog: [docs/research-next-pass.md](docs/research-next-pass.md). Archived greenfield plan: [docs/archive/2026-07-greenfield-implementation-plan.md](docs/archive/2026-07-greenfield-implementation-plan.md).

## License

MIT
