# Robot / machine-mode CLI

Agent-first argv and JSON coaching for `agent-patch`. Patch dialect and matching are unchanged.

## Goals

1. Explicit machine mode: `--robot`, `AGENT_PATCH_ROBOT=1`, or existing `--json` (synonyms).
2. Unique argv rewrite allowlist for known footguns, with top-level `coach` when a rewrite fires.
3. Fail closed with ≥2 copy-paste `examples` when intent is ambiguous; suggest-only for unknown flags.
4. In-tool `robot-docs`; keep AGENTS / skill / help / this doc in sync on flag changes.

## Non-goals

- Hunk fuzzy or patch-body rewriting
- Auto-detect machine mode from non-TTY
- Human-mode argv rewrite
- `capabilities` / `skills get` / bv-style `--robot-*` mega-commands (deferred)
- Move / translate

## Machine mode

Peeked from **raw** argv / env **before** clap so a misplaced `--json` still enables coaching:

- `--json` or `--robot` anywhere in argv, or
- `AGENT_PATCH_ROBOT=1` (or any non-empty value other than `0` / `false` / `off`)

`--robot` implies JSON output (same envelope as `--json`). Keep teaching `--json` in AGENTS/skill; `--robot` is the agent-facing alias.

Stdout: exactly one JSON object. No coach prose on stderr in machine mode.

## Pipeline

1. Peek machine mode.
2. If machine: `argv_normalize` allowlist → `(argv', coach?)` or fail with `examples`.
3. `Cli::try_parse_from(argv')` (never bare `parse()` that exits).
4. Clap / `into_config` failures in machine mode → JSON `INPUT_ERROR` (exit 2), not human stderr.
5. Success with rewrite → attach top-level `coach`.

## Rewrite catalog (auto-apply only when unique)

| ID | Trigger | Rewrite | Coach note |
| --- | --- | --- | --- |
| R1 | `--verify`; ≥2 tokens after `--`; last token is an existing patch-like file (`*** Begin Patch` prefix, or existing `*.patch`) | Move last token to immediately before `--` | `Moved patch path before -- for --verify.` |
| R2 | Subcommand `status\|doctor\|recover\|revert\|gc\|robot-docs`; before it, `--json`/`--robot`/`--quiet` and/or `--root <val>` | Splice those flags immediately after the subcommand | `Moved --json/--root onto the subcommand.` |
| R3 | First non-flag token `undo` + receipt-like positional | `undo` → `revert` | `Alias: undo → revert.` |
| R4 | `--robot` | Treat as machine JSON (`json=true`); no coach required | — |

Fail closed (no rewrite) when the trigger is present but uniqueness fails (missing file, single token after `--`, junk between toplevel flags and subcommand, etc.).

## Fail-closed examples (no auto-rewrite)

| ID | Shape | Examples (canonical forms) |
| --- | --- | --- |
| E1 | `--verify` without argv / without `--verify-shell` | `agent-patch --json --verify -- true < change.patch` · `agent-patch --json --verify change.patch -- true` |
| E2 | `--verify-shell` and trailing argv after `--` | shell-only · argv-only `--verify -- …` |
| E3 | Mode clash (`--check` ⊕ `--plan` ⊕ verify) | one example per intended mode |
| E4 | Invented `--revert` / unknown long flag | `agent-patch revert --json <receipt>` · `agent-patch status --json` |
| E5 | Suspected patch-after-`--` but not unique | same as R1 forms |
| E6 | Toplevel flags + subcommand but unsafe to rewrite | subcommand-local flag examples |

**Suggest-only:** clap unknown-flag failures may include `error.suggestions` (static dictionary / close names). Never auto-apply suggestions.

## JSON fields (contract v2, additive)

| Field | Where | When |
| --- | --- | --- |
| `coach` | Top-level success **and** error | R1–R3 fired: `{ rewrote_from, canonical_argv, note }` |
| `error.examples` | Error body | Fail-closed argv coaching (≥2 strings) |
| `error.suggestions` | Error body | Did-you-mean tokens |
| `hint` | Unchanged | Semantic next-action; not argv tutorials |

`version: 2` when `coach` / `examples` / `suggestions` present. Schema: [cli-json.schema.json](../schemas/cli-json.schema.json).

## `robot-docs`

Subcommand prints a short agent guide (happy paths, footguns, verify/`--` rule, subcommand-local flags). Under machine mode, emit JSON `{ "ok": true, "guide": "…" }`.

## Implementation seat

- Module: `crates/agent-patch/src/argv_normalize.rs` (pure normalize + R1 file probe).
- Wire in `main.rs` before clap; thread `coach` through apply/subcommand JSON emitters in `diagnostics.rs`.
- Fix: `into_config` errors must honor machine mode JSON (today they print human stderr).

## Tests / dogfood

- Unit table for every R* / E* row.
- Integration `tests/robot_argv.rs`: rewrite→success+`coach`; E* → `examples.len() >= 2`; `into_config` + peeked `--json` → JSON stdout.
- Dogfood **T14**: intentional toplevel `--json --root` before `status` (or verify R1) → exit 0 + `coach`.
- Dogfood **T14b** or cargo-only: E1 → non-zero + `examples`.

## Docs sync

Any new flag or rewrite row updates: this file, `--help` / `robot-docs`, README, AGENTS.md, `.cursor/skills/agent-patch/`, `docs/contract-v2.md`, `docs/errors.md`, `docs/schemas/`.
