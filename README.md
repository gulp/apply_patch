# agent-patch

Deterministic, non-interactive CLI for applying localized, transactional file patches. Coding agents use it for safe multi-hunk and multi-file edits without rewriting whole files.

The patch dialect matches the Codex / OpenAI Agents V4A family (`*** Begin Patch` … `*** End Patch`). The runtime is fail-closed: unique exact hunk matching, root confinement, and all-or-nothing commit with rollback.

## Build

```bash
cargo build --release
```

Binary: `target/release/agent-patch`

Repo-local wrapper (release → debug → `cargo run`):

```bash
scripts/agent-patch --help
```

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
| `*** Update File: path` | Contextual hunks (`@@`, ` `, `-`, `+`) |
| `*** Delete File: path` | Remove an existing regular file |

`*** Move File` / `*** Move to:` are not supported in v1.

Paths are repository-relative. Absolute paths, `.` / `..`, and symlink escape outside `--root` are rejected.

Matching is unique and exact. Ambiguous or missing context fails closed (`HUNK_AMBIGUOUS` / `HUNK_NOT_FOUND`). Whitespace-fuzzy and first-match-wins behavior are not used.

Full grammar: [docs/protocol.md](docs/protocol.md). Frozen semantics: [docs/contract-v1.md](docs/contract-v1.md).

## Behavior

1. Parse and validate the entire patch.
2. Snapshot every affected path under the configured root.
3. Apply all updates in memory (locate chunks, then emit).
4. On `--check`, report success or failure and exit without writes.
5. Otherwise revalidate content fingerprints, commit via same-directory temps and atomic rename, and roll back on failure.

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
scripts/dogfood   # end-to-end scenario gate (add/update/delete, stale, ambiguous, path safety)
scripts/bench     # cargo bench (when benches are present)
```

Workspace layout:

```text
crates/agent-patch/   # library + binary
scripts/              # agent-patch, test, lint, dogfood, bench
docs/                 # protocol, contract, errors, design, research
```

Design: [docs/design/](docs/design/). Threat model: [docs/threat-model.md](docs/threat-model.md).

## License

MIT
