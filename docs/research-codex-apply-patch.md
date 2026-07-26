# Research: Codex `apply-patch` vs `diffy` (and Zed ACP)

Primary sources (fetched via `opensrc`):

- `openai/codex` @ main → `~/.opensrc/repos/github.com/openai/codex/main`
- `zed-industries/codex-acp` @ main → `~/.opensrc/repos/github.com/zed-industries/codex-acp/main`
- `bmwill/diffy` 0.5.1 → `~/.opensrc/repos/github.com/bmwill/diffy/0.5.1`

## Verdict

**`codex-apply-patch` does not use `diffy` to apply patches.**

Zed’s `codex-acp` depends on both crates for different jobs:

| Crate | Role in harness |
| --- | --- |
| `codex-apply-patch` | Parse agent patch protocol + apply edits to the filesystem |
| `diffy` | Parse **unified diffs** for **UI display** (old/new text for ACP `Diff` widgets) |

Codex itself uses `diffy` the same way in `codex-tui` (`diff_render.rs`) and cloud-task mocks — display / stats, not apply.

This corrects a common assumption (including our earlier plan wording): “depends on diffy” ≠ “apply engine is diffy.”

## `codex-apply-patch` architecture

Path: `codex-rs/apply-patch/`

```text
parser.rs / streaming_parser.rs  →  AST (Hunk / UpdateFileChunk)
seek_sequence.rs                 →  locate old_lines in file
lib.rs compute_replacements      →  (start, old_len, new_lines)[]
lib.rs apply_replacements        →  in-memory rewrite (apply descending)
lib.rs apply_hunks_to_files      →  FS writes via ExecutorFileSystem
similar::TextDiff                →  observational unified_diff for UI/summary
```

Dependencies of the crate itself (`apply-patch/Cargo.toml`):

- `similar` (not `diffy`)
- `thiserror`, `anyhow`, `tokio`
- `codex-exec-server` (filesystem + sandbox abstraction)
- path URI helpers, tree-sitter bash (invocation / heredoc detection)

### Protocol (same family as `agent-patch`)

Official Lark grammar is documented in `parser.rs`. Supported ops:

- `*** Add File:`
- `*** Delete File:`
- `*** Update File:` + optional `*** Move to:`
- hunks: `@@` / `@@ <change_context>`
- optional `*** End of File`
- optional `*** Environment ID:`

Agent instructions: `codex-rs/prompts/templates/apply_patch_tool_instructions.md`.

Lenient mode strips shell-heredoc wrappers (`<<'EOF'…EOF`) because models sometimes pass heredoc text as a raw argv string under `execvpe`-style tools.

### Matching: `seek_sequence` (intentionally fuzzy)

Order of attempts (first hit wins — **not** unique-match):

1. Exact line equality  
2. Trailing-whitespace-insensitive (`trim_end`)  
3. Leading+trailing trim  
4. Unicode punctuation/space normalisation → ASCII (dashes, quotes, NBSP, etc.)

Also:

- Empty pattern → match at `start` (enables pure insertions)
- `eof` flag prefers matching at end of file
- Trailing empty old-line (final newline sentinel) retried without that sentinel
- Pure-addition chunks (`old_lines` empty) append near EOF

**Contrast with `agent-patch` v1 contract:** we require unique exact matches, reject ambiguity, and forbid whitespace/unicode fuzzy fallback by default. Codex optimises for agent success rate under noisy model output; we optimise for fail-closed safety.

### Apply semantics (important product differences)

From scenario fixtures under `tests/fixtures/scenarios/`:

| Behavior | Codex | `agent-patch` v1 |
| --- | --- | --- |
| Multi-op atomicity | **Not transactional** — `015_failure_after_partial_success_leaves_changes` expects first Add to remain | Validate all in memory; commit with rollback |
| Add existing file | Overwrites (`011_add_overwrites_existing_file`) | `FILE_ALREADY_EXISTS` |
| Move / rename | Supported (`*** Move to:`) | Unsupported (see [design/move.md](./design/move.md)) |
| `*** End of File` | EOF-prefer + fuzz ladder | EOF-prefer exact, then unique forward |
| Pure addition hunk | Allowed (append) | Ambiguous without context → fail (EOF pure-`+` appends at end) |
| Matching | First fuzzy match wins | Unique exact (+ controlled context reduction) |
| Diff library | `similar` for unified diff of result | `similar` for line counts |
| Portable scenarios | Full fixture tree under Codex | Compatible unique-exact subset in `tests/fixtures/codex-scenarios/` |

Codex tracks an `AppliedPatchDelta` with `exact: bool` because writes can partially succeed (e.g. truncate then ENOSPC); delta is used for recovery/accounting rather than full rollback.

### Portable scenario tests (best practice)

Each case is:

```text
NNN_name/
  input/      # optional starting tree
  patch.txt
  expected/   # final tree
```

README states the suite is meant to be portable across languages. `agent-patch` runs a unique-exact-compatible subset via `tests/codex_scenarios.rs` (exclusions documented in that fixture README).

## Zed `codex-acp` integration

Pinned to Codex git tag `rust-v0.137.0`:

```toml
codex-apply-patch = { git = "https://github.com/openai/codex", tag = "rust-v0.137.0" }
diffy = { version = "0.5.0", features = ["std"] }
```

Usage:

1. **`parse_patch`** on tool input → build ACP `ToolCallContent::Diff` previews from hunk `old_lines`/`new_lines` (no filesystem apply in this path).
2. **`diffy::Patch::from_str(unified_diff)`** when the harness already has a unified diff from Codex file-change events → reconstruct old/new per hunk for the UI; fall back to plain text if parse fails.

So ACP is a real agent-harness editing path: it **displays** patches via both parsers and **executes** via Codex core/`apply_patch`, not via `diffy::apply`.

## What `diffy` actually provides

`diffy` is a unified-diff create/parse/apply/merge library. `diffy::apply` patches a base string from a parsed unified `Patch`. That is a different dialect than Codex’s `*** Begin Patch` envelope.

Using `diffy::apply` as the engine for the Codex/agent-patch protocol would require translating typed hunks into unified diffs (or reparsing), and would inherit unified-diff location/fuzz semantics — which both Codex and our contract deliberately avoid exposing as the agent protocol.

## Practices reflected in `agent-patch`

1. **Protocol-native matcher** — custom unique-exact locate/emit; `similar` only for observational summaries (Codex pattern).
2. **No silent fuzzy default** — whitespace/unicode fuzz stays out of the default path; any future `--fuzzy` mode would still require uniqueness.
3. **Codex-style scenario fixtures** — compatible unique-exact subset under `tests/fixtures/codex-scenarios/`.
4. **Transactional multi-file commit** — validate in memory; commit with rollback (delta vs Codex partial leave).
5. **Display vs apply split** — `diffy`/`similar` may parse *resulting* unified diffs for UI; they are not the V4A apply engine (same split as Zed ACP).
6. **Protocol surface** — `*** End of File` and `@@ <anchor>` are in the dialect; `*** Move to:` and streaming parse remain backlog ([research-next-pass.md](./research-next-pass.md), [design/move.md](./design/move.md)).
7. **Abstract filesystem** — `FileSystem` trait for commit/rollback and fault injection.
8. **Agent instructions** — `AGENTS.md` / `CLAUDE.md` teach fail-closed unique matching and `scripts/agent-patch` usage.

## Source map

| Topic | Path |
| --- | --- |
| Apply + replacements | `codex-rs/apply-patch/src/lib.rs` |
| Fuzzy seek | `codex-rs/apply-patch/src/seek_sequence.rs` |
| Grammar / AST | `codex-rs/apply-patch/src/parser.rs` |
| Streaming parse | `codex-rs/apply-patch/src/streaming_parser.rs` |
| Scenario E2E | `codex-rs/apply-patch/tests/fixtures/scenarios/` |
| Agent instructions | `codex-rs/prompts/templates/apply_patch_tool_instructions.md` |
| ACP parse for UI | `codex-acp/src/thread.rs` (`parse_apply_patch_call`, `extract_tool_call_content_from_unified_diff`) |
| Codex TUI diffy display | `codex-rs/tui/src/diff_render.rs` |
| diffy apply API | `diffy/src/apply.rs` |

## Related: OpenAI Agents Python

See [research-openai-agents-apply-diff.md](./research-openai-agents-apply-diff.md) for the Python `apply_diff` / V4A pure-text applicator used by `ApplyPatchTool` and sandbox editors. Same protocol family; complementary design notes for newline handling and chunk application.