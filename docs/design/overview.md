# Overview

## Problem

Agents need localized multi-file edits that are model-legible (V4A), fail-closed (no wrong edit, no partial tree), and runnable as one CLI (`scripts/agent-patch`).

## Pipeline

```text
CLI (clap) → app::run
               ├─ parse_patch          (protocol only)
               ├─ path policy + snapshot
               ├─ validate + plan
               ├─ apply_update         (pure text)
               └─ commit_plan          (FS; skipped for --check)
```

| Module | Knows | Must not know |
| --- | --- | --- |
| Protocol | V4A grammar, spans | FS, matching |
| Path / snapshot | Root, symlinks, bytes, blake3, newlines | Hunk syntax |
| Apply engine | Locate + emit text | Paths, writes, JSON |
| Commit | Temps, rename, rollback | Patch dialect |
| CLI | Flags, streams, exit codes | Match algorithms |

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

1. Unique exact match → `HUNK_NOT_FOUND` / `HUNK_AMBIGUOUS` (no first-match-wins).
2. No default rstrip/strip/unicode fuzz (optional `--fuzzy` is v1.1+ only, still unique).
3. Full in-memory apply then transactional commit + rollback (vs Codex/Agents sequential writes).
4. Add never overwrites.
5. `*** Move to:` and `*** End of File` deferred (protocol-compatible later; see [`../research-next-pass.md`](../research-next-pass.md)).
6. Observational diffs via `similar` only.

## Data flow

```text
patch bytes → limits → parse → path validate → snapshot → op↔state validate
  → in-memory apply (Update/Add/Delete plan) → PATCH_NO_EFFECT check
  → --check? emit and stop
  → revalidate blake3 → prepare temps → commit → rollback on failure → emit
```

## Public surface

```text
scripts/agent-patch [--check] [--json] [--quiet] [--root PATH]
                    [--max-files N] [--max-patch-bytes N] [--max-file-bytes N]
                    [PATCH_FILE]
```

Exits / codes: [`../errors.md`](../errors.md), [`../contract-v1.md`](../contract-v1.md).

## Non-goals

AST transforms; fuzzy default; Git stage/commit; MCP requirement; binary files; interactive conflict UI; whole-file rewrite fallback; `diffy`/`flickzeug` as V4A apply backend.

## Harmful patterns

- Inferring apply algorithm from a dependency name (`diffy` in Codex workspace ≠ apply).
- Feeding V4A text to `diffy::apply` / `flickzeug::apply` (unified-diff APIs).
- Rematching against a buffer mutated after each hunk (prefer locate-all → emit).
- Claiming multi-file FS atomicity without rollback (ordinary FS has none).
- Implementing Move/EOF without contract bump and commit-order tests.

## Deferred (fact-backed backlog)

| Item | Why it matters |
| --- | --- |
| `*** End of File` | In every major V4A grammar; EOF-preferring locate |
| `*** Move to:` | Codex/Agents/OpenClaw/OpenCode; needs collision + rollback design |
| Locate-all → cursor emit refactor | Matches Agents/Codex replacement model; simplifies overlap |
| Codex scenario corpus (exact-only subset) | Shared dialect tests |
| Optional path list helper | OpenClaw-style preflight for harnesses |
