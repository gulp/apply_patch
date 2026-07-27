# agent-patch

Deterministic, non-interactive CLI for applying localized, transactional file patches. Coding agents use it for safe multi-hunk and multi-file edits without rewriting whole files.

The patch dialect matches the Codex / OpenAI Agents V4A family (`*** Begin Patch` … `*** End Patch`). The runtime is fail-closed: unique hunk matching (exact by default; optional unique-only fuzz), root confinement, durable journaled commit with content-addressed before-images, crash recovery, verify-gated promote, and self-contained receipts/revert.

Baseline matching and ops: [docs/contract-v1.md](docs/contract-v1.md). Plans, verify, journals, receipts, fuzzy/risk/idempotent: [docs/contract-v2.md](docs/contract-v2.md).

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

This is the canonical entrypoint for agents and humans. Prefer it in docs and scripts. Rebuild release after engine changes before trusting the wrapper (`scripts/agent-patch doctor`).

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
agent-patch [OPTIONS] [PATCH_FILE] [-- <VERIFY_ARGV>...] [COMMAND]
```

If `PATCH_FILE` is omitted, the patch is read from stdin. Modes `--check`, `--plan`, and `--verify` are mutually exclusive with each other (and with a plain mutating apply).

### Apply / check / plan / verify options

| Option | Description |
| --- | --- |
| `--check` | Validate and apply in memory; write nothing |
| `--plan` | Emit immutable `ExecutionPlan` + diffs as JSON-friendly output; write nothing |
| `--verify` | Materialize a shadow workspace, run argv after `--`, promote to the real root only on exit 0 |
| `--shadow-mode <tree\|touched>` | Shadow policy (`tree` default = representative under excludes; `touched` = planned paths only, non-representative) |
| `--shadow-include-caches` | Include build/cache dirs in a tree shadow (still budgeted) |
| `--fuzzy <off\|rstrip\|strip>` | Unique-only fuzzy ladder for locate (default `off`) |
| `--risk <off\|warn\|refuse>` | Match-risk gate over `MatchEvidence` (default `off`) |
| `--idempotent` | Success when the full intended after-state is already present; mixed partial replay → `PARTIALLY_APPLIED` |
| `--receipt <PATH>` | On successful mutating apply/verify, also export the receipt JSON to this path |
| `--root <PATH>` | Repository root (default: current directory) |
| `--json` | Emit exactly one JSON object on stdout |
| `--quiet` | Suppress the human success summary |
| `--max-files <N>` | Max files per patch (default: 128) |
| `--max-patch-bytes <N>` | Max patch size (default: 4 MiB) |
| `--max-file-bytes <N>` | Max size of any target file (default: 16 MiB) |
| `-h`, `--help` | Help |
| `-V`, `--version` | Version |

Verify argv (only with `--verify`):

```bash
scripts/agent-patch --verify -- cargo test -q < change.patch
scripts/agent-patch --verify -- true < change.patch
```

### Subcommands

| Command | Description |
| --- | --- |
| `status [--json] [--root PATH]` | Lock / journal / object-store health (no mutation); non-zero if incomplete journals |
| `doctor [--json] [--root PATH]` | PATH resolution, binary freshness vs sources, store/journal/shadow health |
| `recover [--transaction ID] [--json] [--root PATH]` | Resolve incomplete transaction journals to proven all-before or all-after |
| `revert <RECEIPT> [--json] [--root PATH]` | Inverse journaled transaction from a self-contained receipt |
| `gc [--dry-run] [--json] [--root PATH]` | Reference-safe GC of unreferenced before-image objects |

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

Emit an execution plan (no writes):

```bash
scripts/agent-patch --plan --json < change.patch
```

Apply with receipt export, then revert:

```bash
scripts/agent-patch --receipt /tmp/r.json < change.patch
scripts/agent-patch revert /tmp/r.json
```

Verify-gated apply:

```bash
scripts/agent-patch --verify -- cargo check -q < change.patch
```

Hash pin (fails closed before locate if the on-disk file differs):

```bash
scripts/agent-patch <<'PATCH'
*** Begin Patch
*** Update File: src/config.rs
*** Hash: blake3 <64-hex-digest-of-current-file-bytes>
@@
-old
+new
*** End Patch
PATCH
```

Operational:

```bash
scripts/agent-patch status --json
scripts/agent-patch doctor --json
scripts/agent-patch recover --json
scripts/agent-patch gc --dry-run --json
```

## Protocol

Envelope:

```text
*** Begin Patch
<operations>
*** End Patch
```

| Operation | Role |
| --- | --- |
| `*** Add File: path` | Create a missing file (`+` content lines) |
| `*** Update File: path` | Contextual hunks; optional `*** Hash: blake3 <hex>` pin; optional `*** End of File` |
| `*** Delete File: path` | Remove an existing regular file |

`*** Move File` / `*** Move to:` are not supported (design notes: [docs/design/move.md](docs/design/move.md)).

### Update matching

- Hunks start with bare `@@` or `@@ <anchor>` (unique exact line at/after the search cursor). Unified-diff numeric headers such as `@@ -1,3 +1,4 @@` are ignored as location math; the body still matches by content.
- Default matching is unique and exact on logical lines. Ambiguous or missing context fails closed (`HUNK_AMBIGUOUS` / `HUNK_NOT_FOUND`).
- Optional `--fuzzy=rstrip|strip` extends the ladder after exact/context-reduction failure; every level still requires a unique hit (never first-match-wins).
- Optional trailing `*** End of File` prefers an exact match aligned at EOF, then falls back to unique forward search.
- Controlled edge-context reduction applies only when the reduced needle remains unique.
- Apply resolves all hunks on the original line array, then emits with a forward cursor (no rematch against a mutating buffer).
- On update, the file’s LF or CRLF style wins; mixed endings are rejected. Add File content uses LF.

Paths are repository-relative. Absolute paths, `.` / `..`, and symlink escape outside `--root` are rejected.

Full grammar: [docs/protocol.md](docs/protocol.md). Contracts: [docs/contract-v1.md](docs/contract-v1.md), [docs/contract-v2.md](docs/contract-v2.md).

## Behavior

1. Parse and validate the entire patch (optional hash pins checked against snapshots before locate).
2. Snapshot every affected path under the configured root.
3. With `--idempotent`, prove full after-state already present → success (`already_applied`) without mutation; mixed partial replay → `PARTIALLY_APPLIED`.
4. Build an immutable execution plan (`locate_chunks` → `emit_chunks`, optional fuzzy/risk).
5. `--check` / `--plan`: report and exit without writes (`--plan` includes plan digest + diffs).
6. `--verify`: materialize a shadow (default `tree` with cache excludes), run the argv command with `cwd` = shadow root, discard the shadow and leave the real root unchanged on failure/timeout/signal; on success, promote via the same journaled commit path as apply.
7. Mutating apply / promote: acquire `.agent-patch/lock`, refuse incomplete journals, store before-image objects, write a durable journal (`PREPARED` → `COMMITTING` → `COMPLETED`), commit via same-directory temps and atomic rename, finalize an internal receipt under `.agent-patch/receipts/`.

`--check` and `--plan` never create temps or mutate the tree. Hard links are never used for shadows or rollback storage.

### `.agent-patch/` store

```text
.agent-patch/
├── lock
├── objects/<blake3>              # immutable before-images
├── transactions/<txid>/journal.json
├── receipts/<txid>.json
└── shadows/                      # verify workspaces (removed on success)
```

Details: [docs/design/transaction-journal.md](docs/design/transaction-journal.md), [docs/schemas/](docs/schemas/).

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | Success (including `already_applied` under `--idempotent`) |
| 1 | Patch does not apply / verify failed / risk refused / partially applied |
| 2 | Malformed or unsupported patch / invalid receipt |
| 3 | Filesystem / I/O failure |
| 4 | Unsafe path or policy violation |
| 5 | Concurrent modification / hash pin mismatch / root locked / stale revert |
| 6 | Internal invariant / rollback failed / recovery required or ambiguous |
| 7 | Configured resource or shadow limit exceeded |

Granular codes (`HUNK_NOT_FOUND`, `RECOVERY_REQUIRED`, `VERIFY_TIMEOUT`, …): [docs/errors.md](docs/errors.md).

## Output

**Human mode (default):** concise success summary on stdout; diagnostics on stderr.

**JSON mode (`--json`):** one JSON object on stdout (`ok: true` with summary/files, or `ok: false` with `error`). Plan/verify/oracle fields use `version: 2` when present. Content fingerprints use BLAKE3. Successful applies may include `transaction_id`; `--idempotent` successes may set `already_applied: true`. Errors may include `candidates`, `hint`, `root_changed`, and `recovery_required`.

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
| `cargo test --workspace` | Unit + CLI integration (atomicity, concurrency, path safety, limits, journal/receipt/verify) |
| `tests/codex_scenarios.rs` | Portable Codex fixture subset under `tests/fixtures/codex-scenarios/` |
| `scripts/dogfood` | Acceptance gate; rebuilds release before running |
| `fuzz/` | `cargo-fuzz` targets: `parse_patch`, `path_policy`, `apply_update` (see [fuzz/README.md](fuzz/README.md)) |
| `benches/apply_update.rs` | Locate→emit throughput on a multi-thousand-line buffer |

CI (Linux and macOS): `fmt` + `clippy -D warnings` + `cargo test --workspace` + `scripts/dogfood`.

Workspace layout:

```text
crates/agent-patch/   # library + binary (+ benches, integration tests)
fuzz/                 # cargo-fuzz workspace (parse / path / apply)
scripts/              # agent-patch, test, lint, dogfood, bench
docs/                 # protocol, contracts, errors, design, research, schemas
```

Design: [docs/design/](docs/design/). Threat model: [docs/threat-model.md](docs/threat-model.md). Active plan: [IMPLEMENTATION_PLAN.md](IMPLEMENTATION_PLAN.md). Seam ground truth: [docs/research-post-v1-seams.md](docs/research-post-v1-seams.md). Backlog: [docs/research-next-pass.md](docs/research-next-pass.md). Archived greenfield plan: [docs/archive/2026-07-greenfield-implementation-plan.md](docs/archive/2026-07-greenfield-implementation-plan.md).

## License

MIT
