# Overview

## Problem

Agents need localized multi-file edits that are model-legible (V4A), fail-closed (no wrong edit, no partial tree), crash-recoverable, and runnable as one CLI (`scripts/agent-patch`).

## Pipeline

```text
CLI (clap) → app::run
               ├─ parse_patch          (protocol only; optional hash pins)
               ├─ path policy + snapshot
               ├─ idempotent assess (optional)
               ├─ validate + plan (fuzzy / risk)
               ├─ apply_update         (pure text)
               ├─ --check / --plan     (stop; no FS writes)
               ├─ --verify / --verify-shell (shadow + command; promote on success)
               └─ commit_plan          (lock, objects, journal, rename, receipt)
```

| Module | Knows | Must not know |
| --- | --- | --- |
| Protocol | V4A grammar, spans, hash pins | FS, matching |
| Path / snapshot | Root, symlinks, bytes, blake3, newlines | Hunk syntax |
| Apply engine | Locate + emit text, fuzzy ladder | Paths, writes, JSON |
| Match opts / risk | Evidence gates | FS |
| Shadow / verify | Tree copy, process group, budgets | Patch dialect |
| Commit / journal / objects / receipt | Temps, rename, rollback, CAS, recover | Patch dialect |
| CLI | Flags, subcommands, streams, exit codes | Match algorithms |

## Ground truth (verified)

| Fact | Source |
| --- | --- |
| V4A markers: `*** Begin/End Patch`, Add / Update / Delete; optional `*** Move to:`, `*** End of File` | Codex `codex-rs/apply-patch/src/parser.rs`; Agents `apply_patch_tool.py`; Aider `patch_coder.py` |
| Codex apply uses custom `seek_sequence` + `similar` for unified summaries — **not** `diffy` | `codex-rs/apply-patch/Cargo.toml`, `lib.rs`, `seek_sequence.rs` |
| Codex workspace `diffy` is for TUI / mock unified-diff **display** | `codex-rs/tui/src/diff_render.rs` |
| Zed `codex-acp` depends on both: `parse_patch` for tool UI; `diffy::Patch::from_str` for unified-diff UI | `codex-acp/Cargo.toml`, `src/thread.rs` |
| Agents Python/JS `apply_diff` / `applyDiff`: headerless per-file body; locate on original lines; forward cursor emit; create mode = `+` lines only | `openai-agents-python/src/agents/apply_diff.py`; `openai-agents-js/.../applyDiff.ts` |
| Agents Python: file newline wins on update (LF↔CRLF); JS always rejoins with `\n` | Python `test_apply_diff.py` vs JS `applyDiff.ts` |
| Codex/Agents/Aider: first-match + whitespace/unicode fuzz ladders | `seek_sequence.rs`; Agents `_find_context_core`; Aider `find_context_core` |
| Codex is **not** multi-op transactional; scenario `015_*leaves_changes` | `codex-rs/apply-patch/tests/fixtures/scenarios/` |
| Codex Add may overwrite; we reject with `FILE_ALREADY_EXISTS` | Codex scenario `011_add_overwrites_*` vs our contract |
| `flickzeug` = maintained `diffy` fork; unified-diff + `FuzzyConfig`; used by rattler-build / cargo-mutants — **not** V4A | `crates:flickzeug` `src/apply.rs`, README |
| Portable fixtures: `input/` + `patch.txt` + `expected/` | Codex `tests/fixtures/scenarios/README.md` |

## Product choices (deliberate deltas)

1. Unique match → `HUNK_NOT_FOUND` / `HUNK_AMBIGUOUS` (no first-match-wins).
2. Default exact locate; optional `--fuzzy=rstrip|strip` is unique-only (never default).
3. Full in-memory plan then journaled commit + objects + receipt (vs Codex/Agents sequential writes).
4. Add never overwrites.
5. `*** End of File` with EOF-prefer exact locate; `*** Move to:` deferred ([`move.md`](move.md)).
6. Observational diffs via `similar` only.
7. Locate all chunks on the original lines, then forward-cursor emit (`engine/locate.rs`, `engine/emit.rs`).
8. `--verify` / `--verify-shell` use a representative tree shadow under documented excludes; hard links forbidden.
9. Incomplete journals block mutation until `recover`.

## Data flow

```text
patch bytes → limits → parse → path validate → snapshot → op↔state validate
  → optional idempotent assess
  → in-memory plan (Update/Add/Delete) → risk gate → PATCH_NO_EFFECT check
  → --check / --plan? emit and stop
  → --verify / --verify-shell? shadow → command → on success continue
  → lock → refuse incomplete journals → put before-images → journal PREPARED
  → COMMITTING → rename/delete → COMPLETED → receipt → emit
```

## Public surface

```text
scripts/agent-patch [--check|--plan|--verify|--verify-shell SCRIPT]
                    [--fuzzy …] [--risk …] [--idempotent]
                    [--shadow-mode tree|touched] [--shadow-include-caches]
                    [--receipt PATH] [--json] [--quiet] [--root PATH]
                    [--max-files N] [--max-patch-bytes N] [--max-file-bytes N]
                    [PATCH_FILE] [-- <VERIFY_ARGV>…]

scripts/agent-patch status|doctor|recover|revert|gc …
```

Exits / codes: [`../errors.md`](../errors.md), [`../contract-v1.md`](../contract-v1.md), [`../contract-v2.md`](../contract-v2.md).

## Non-goals

AST transforms; fuzzy default; Git stage/commit; MCP requirement; binary files; interactive conflict UI; whole-file rewrite fallback; `diffy`/`flickzeug` as V4A apply backend; hard-link shadows; hashes-only receipts.

## Harmful patterns

- Inferring apply algorithm from a dependency name (`diffy` in Codex workspace ≠ apply).
- Feeding V4A text to `diffy::apply` / `flickzeug::apply` (unified-diff APIs).
- Rematching against a buffer mutated after each hunk (prefer locate-all → emit).
- Claiming multi-file FS atomicity without journaled recoverability (ordinary FS has none).
- Implementing Move without contract bump and commit-order tests (see [`move.md`](move.md)).
- Deleting incomplete journals via stale-lock heuristics.

## Deferred (fact-backed backlog)

| Item | Why it matters |
| --- | --- |
| `*** Move to:` | Codex/Agents/OpenClaw/OpenCode; collision + rollback — [`move.md`](move.md) |
| Optional path list helper | OpenClaw-style preflight for harnesses |
| Streaming patch parse | Codex `streaming_parser.rs`; only if >`max_patch_bytes` streaming is required |
| Unicode fuzzy normalize | Upstream ladder only; not enabled |
