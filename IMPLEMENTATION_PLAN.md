# `agent-patch` — Implementation Plan

Status: Active post-v1 plan (agent reliability, recovery, and verification)
Supersedes: [docs/archive/2026-07-greenfield-implementation-plan.md](docs/archive/2026-07-greenfield-implementation-plan.md)
Authoritative behavior today: [README.md](README.md), [docs/contract-v1.md](docs/contract-v1.md), [docs/protocol.md](docs/protocol.md), [docs/design/](docs/design/)
Primary users: Coding agents operating through shell-capable harnesses
Primary interface: Repo-local command-line executable (`scripts/agent-patch`; optional PATH via direnv / `cargo install`)
Implementation language: Rust
Initial platforms: Linux and macOS
Primary objective: Keep fail-closed V4A apply, and make agent recovery, verification, and multi-agent safety first-class

---

## 1. Goals / Non-goals

### 1.1 Goals

1. Preserve the v1 contract: unique-exact locate→emit, transactional commit, root confinement, stable exits/JSON, no silent fuzzy default.

2. Make every apply failure **actionable for an agent** without human intervention:
   - candidate locations and excerpts;
   - draft repair patches where safe;
   - explicit next-action hints tied to `ErrorCode`.

3. Support **verify-gated commit**: apply to a shadow tree, run a user command, promote only on success.

4. Emit **apply receipts** and support transactional **revert** from a receipt.

5. Support optional **content-hash pins** on patch targets so parallel agents fail closed on stale bases before hunk matching.

6. Support **idempotent / already-applied** detection as an explicit success mode (not silent no-op confusion with `PATCH_NO_EFFECT`).

7. Offer **unique-only** opt-in fuzz (`rstrip` / `strip`) that never selects first-match-wins.

8. Detect **wrong-match risk** (thin context / near-miss twins) and refuse or warn before commit.

9. Provide structured **`--plan`** output (locate results + diffs) without writing the tree.

10. Ship **`doctor`** for PATH, direnv, and release-binary freshness footguns.

11. Keep invocation agent-friendly:
    - `scripts/agent-patch` canonical;
    - bare `agent-patch` when `scripts/` is on `PATH`;
    - no MCP requirement.

12. Extend the dialect only via explicit contract bumps (`docs/contract-v1.md` or a successor).

13. Keep Move File out of default path until [docs/design/move.md](docs/design/move.md) is implemented under a contract bump.

14. Keep dogfood fixtures and Codex scenario subsets green on Linux and macOS CI.

### 1.2 Non-goals

1. Not a general-purpose editor, AST refactor engine, or NL→patch generator.

2. Not silent fuzzy first-match, whole-file rewrite recovery, or `diffy`/`flickzeug` as V4A apply backends.

3. Not automatic semantic merge resolution.

4. Not Git stage/commit/PR creation (hooks that *call* verify commands are allowed; Git ownership is not).

5. Not interactive TUIs or conflict UIs.

6. Not binary-file patching or arbitrary encodings beyond UTF-8 (+ BOM preserve).

7. Not streaming multi-MiB patch parse unless a concrete harness requires it.

8. Not a Responses API server; optional JSON op bridge may wrap envelopes later without replacing them.

9. Not overwriting Add or Move-dest collision (deliberate stricter delta vs Codex).

10. Not default `--fuzzy`; uniqueness remains mandatory at every fuzz level.

---

## 2. Product Definition

### 2.1 Project

`agent-patch`

A repo-local Rust CLI that applies structured, localized, transactional V4A-family patches for coding agents—with verification, recoverable failures, and multi-agent freshness controls.

### 2.2 Users

Primary:

- Claude Code / Cursor / Codex-style agents;
- shell-capable autonomous coding agents;
- humans replaying or reviewing agent patches;
- CI validating patches (`--check`, `--plan`, `--verify`).

Secondary:

- harness authors;
- multi-agent orchestrators;
- maintainers dogfooding via `fixtures/dogfood` and `scripts/dogfood`.

### 2.3 Problem statement

v1 solves whole-file rewrite risk with fail-closed unique-exact apply. Agents still lose time when:

- failures do not include enough context to regenerate a patch;
- “unique” matches the wrong near-duplicate;
- apply succeeds but breaks the build;
- retries re-apply and confuse `PATCH_NO_EFFECT` with success;
- parallel agents race without content pins;
- stale `target/release` binaries silently run old code.

This plan addresses those gaps without abandoning fail-closed semantics.

### 2.4 Constraints

- Contract-first: bump docs before behavior.
- Pure engine: locate/emit stay FS-free.
- Commit stays all-or-nothing with rollback.
- JSON remains versioned and machine-pure.
- Complexity budget: prefer features that reuse `locate_chunks`, `emit_chunks`, `commit_plan`, and `PublicError`.

### 2.5 Environment

- Linux and macOS (CI matrix).
- Rust stable workspace under `crates/agent-patch`.
- Invocation: `scripts/agent-patch` (release → debug → `cargo run`); optional `PATH_add scripts` via `.envrc`.
- Dogfood tree: `fixtures/dogfood` (never required for production use).

---

## 3. User-Facing Contract

### 3.1 Canonical invocation

```bash
scripts/agent-patch [OPTIONS] [PATCH_FILE]
# or, with scripts/ on PATH:
agent-patch [OPTIONS] [PATCH_FILE]
```

### 3.2 Command surface (v1 + planned)

Existing:

| Flag / arg | Role |
| --- | --- |
| `PATCH_FILE` / stdin | Patch input |
| `--check` | Validate + in-memory apply; no writes |
| `--root` | Repository root |
| `--json` | Single JSON object on stdout |
| `--quiet` | Suppress human success summary |
| `--max-files` / `--max-patch-bytes` / `--max-file-bytes` | Limits |

Planned (contract bump required where noted):

| Flag / subcommand | Role | Contract |
| --- | --- | --- |
| `--plan` | Locate→emit preview + structured diffs; no writes | Additive CLI |
| `--verify <CMD>` | Shadow apply; run command; promote on exit 0 | Additive; document shadow semantics |
| `--receipt <PATH>` | Write apply receipt after success | Additive |
| `--revert <RECEIPT>` | Transactional undo from receipt | Additive |
| `--fuzzy <off\|rstrip\|strip>` | Unique-only fuzz ladder; default `off` | Contract bump for matching |
| `--risk <off\|warn\|refuse>` | Thin-context / near-miss gate | Additive policy |
| `--idempotent` | Treat already-applied as success | Contract bump for success modes |
| `doctor` | Env / PATH / binary freshness | Additive |
| `translate` | V4A ↔ unified (optional phase) | Additive; not apply path |

### 3.3 Protocol (baseline)

Frozen in [docs/protocol.md](docs/protocol.md):

- Envelope `*** Begin Patch` … `*** End Patch`
- `Add File` / `Update File` / `Delete File`
- `@@` / `@@ <anchor>`; optional `*** End of File`
- Unique exact locate→emit; EOF-prefer when marked
- Move deferred ([docs/design/move.md](docs/design/move.md))

Planned protocol extensions (explicit bump):

```text
*** Hash: blake3 <hex>     # optional per-file pin before hunks / after Update header
```

Exact grammar to freeze in Phase 0 of this plan.

### 3.4 Matching contract (baseline + deltas)

Baseline: [docs/contract-v1.md](docs/contract-v1.md).

Deltas under bump:

1. `--fuzzy=rstrip|strip`: normalize for search only; accept only if **exactly one** match.
2. Risk gate: if accepted match has near-miss siblings under reduced context, `warn` or `refuse`.
3. Idempotent: if old-side absent and new-side present at expected locus, success with `already_applied`.

### 3.5 Stdout / stderr

Unchanged philosophy:

- human mode: summary stdout, diagnostics stderr;
- `--json`: one object stdout; stderr empty for structured failures.

### 3.6 Exit taxonomy

Keep exits 0–7. Planned additive meanings stay within classes:

| Code | Class | Notes |
| --- | --- | --- |
| 0 | success | includes idempotent already-applied when enabled |
| 1 | does not apply | hunk / risk-refuse / verify-cmd failed after clean shadow discard |
| 2 | malformed / unsupported | |
| 3 | I/O | |
| 4 | path policy | |
| 5 | concurrent / hash pin mismatch | |
| 6 | internal / rollback / revert failed | |
| 7 | limits | |

---

## 4. Architecture

### 4.1 Layered architecture

```text
CLI (clap)
  → app::run
       ├─ doctor | translate | revert   (optional entrypoints)
       ├─ parse_patch (+ optional hash pins)
       ├─ path policy + snapshot
       ├─ validate + plan
       ├─ risk / fuzzy / idempotent (pure)
       ├─ apply_update (locate_chunks → emit_chunks)
       ├─ --check / --plan → emit and stop
       ├─ --verify → shadow FS → command → promote or discard
       └─ commit_plan → receipt
```

### 4.2 Components

#### 4.2.1 CLI adapter

Parse flags/subcommands; enforce mutually exclusive modes (`--check` vs `--verify` vs `--revert`).

#### 4.2.2 Protocol parser

Existing AST + EOF + anchors. Extend for optional hash pins.

#### 4.2.3 Path policy / snapshot / validate / plan

Unchanged seams; snapshot gains fields needed for idempotent detection and risk.

#### 4.2.4 Engine

`matcher` / `locate` / `emit` / `apply` / `diff_summary`. Fuzzy and risk are locate-time policies.

#### 4.2.5 Shadow filesystem

New adapter implementing the same `FileSystem` trait over a temp root mirroring relative paths.

#### 4.2.6 Verify runner

Spawn user command with `cwd=shadow` or `cwd=real` + env pointing at shadow—**Phase 0 decision**. Default recommendation: `cwd` = shadow root so tools see the candidate tree.

#### 4.2.7 Receipt store

Serialize plan + before/after hashes + inverse ops; load for revert.

#### 4.2.8 Diagnostics

Extend JSON error objects with `candidates`, `excerpts`, optional `repair_patch`.

#### 4.2.9 Doctor

Read mtimes of `src/**` vs `target/release/agent-patch`; check `command -v agent-patch`; print remediation.

### 4.3 Boundary rules

- Engine never runs verify commands.
- Commit never fuzzy-matches.
- Repair patches are suggestions only; never auto-applied.
- Shadow trees are deleted on success promote and on failure (optional `--keep-shadow` later).

---

## 5. Core Invariants

Carry forward v1 invariants I1–I18 (root confinement, transactionality, no mutation before validation, no silent fallback, unique matching, …) from the archived plan and [docs/design/overview.md](docs/design/overview.md).

### I19 — Verify before promote

`--verify` never mutates the real root unless the verify command exits 0 and revalidation still passes.

### I20 — Receipt fidelity

Revert applies the inverse of a successful receipt or fails closed with no partial undo.

### I21 — Fuzzy uniqueness

Any fuzz level still requires exactly one match; zero or many → existing hunk errors.

### I22 — Oracle honesty

Candidate lists and repair patches must be derived from the same locator used for apply; no second guessed algorithm.

### I23 — Hash pin precedence

When a pin is present, pin failure precedes hunk matching.

### I24 — Mode exclusivity

`--check`, `--plan`, `--verify`, apply, and `--revert` do not silently combine conflicting write behaviors.

---

## 6. Data Flow

### 6.1 Normal apply

```text
input → limits → parse → path → snapshot → validate
  → (hash pins) → locate/emit → risk → plan
  → revalidate → commit → receipt? → emit success
```

### 6.2 Check / plan

Same through in-memory apply; no temps; `--plan` adds structured hunk/diff payload.

### 6.3 Verify

```text
… → in-memory plan → materialize shadow → run CMD
  → fail: discard shadow, exit 1 (verify failed)
  → ok: revalidate real root → commit → receipt? → cleanup shadow
```

### 6.4 Revert

```text
load receipt → verify current hashes match receipt.after
  → build inverse plan → commit → emit
```

### 6.5 Failure

No real-root mutation on validation/locate/risk/verify failure. Rollback on mid-commit failure (existing).

---

## 7. Data Model and Schemas

### 7.1 Success JSON (extensions)

Additive fields (version stays `1` or bump to `2` if breaking—**Phase 0**):

```json
{
  "version": 1,
  "ok": true,
  "mode": "apply|check|plan|verify|revert",
  "already_applied": false,
  "summary": {},
  "files": [],
  "plan": null,
  "receipt_path": null,
  "verify": { "command": "...", "exit_code": 0, "duration_ms": 0 }
}
```

### 7.2 Error JSON (extensions)

```json
{
  "ok": false,
  "error": {
    "code": "HUNK_AMBIGUOUS",
    "exit_code": 1,
    "message": "...",
    "path": "...",
    "candidates": [{ "start_line": 10, "end_line": 12, "excerpt": "..." }],
    "repair_patch": "*** Begin Patch\n...",
    "hint": "..."
  }
}
```

### 7.3 Receipt schema

```json
{
  "version": 1,
  "root": "...",
  "created_at": "...",
  "files": [
    {
      "path": "src/a.rs",
      "operation": "update|add|delete",
      "before_blake3": "...",
      "after_blake3": "...",
      "before_bytes_b64": null,
      "after_bytes_b64": null
    }
  ]
}
```

Phase 0 decides whether receipts embed full before bytes (size-bounded) or re-read strategy for revert.

### 7.4 Fingerprints / newlines

Unchanged: BLAKE3 labeled; LF/CRLF file-wins; mixed rejected on update.

---

## 8. Filesystem Transaction Strategy

### 8.1 Real-root commit

Unchanged: revalidate → temps → rename → rollback ([docs/design/transaction.md](docs/design/transaction.md)).

### 8.2 Shadow materialization

- Create exclusive temp directory.
- Write full planned file set (add/update) and record deletes as absences in shadow view.
- Tools that expect a full checkout may need copy-on-write of unread files—**Phase 0**:  
  - **A (simpler):** shadow contains only touched paths (verify cmds must be path-local);  
  - **B (heavier):** overlay or worktree clone.  
  Recommendation: start with **A** + document constraints; offer worktree mode later.

### 8.3 Promote

After verify 0: run existing commit against real root (do not rename shadow into root).

### 8.4 Honest claims

Never advertise multi-file atomic visibility beyond rollback guarantees.

---

## 9. Matching and Patch Application

### 9.1 Baseline

`locate_chunks` → `emit_chunks`; anchors; EOF-prefer; context reduction when unique.

### 9.2 Fuzzy unique ladder

```text
off → exact
rstrip → trim_end equality, unique
strip → trim equality, unique
```

No unicode punctuation normalize in the first fuzzy ship (Codex has it; defer to keep scope small).

### 9.3 Risk gate

For each accepted chunk, compute match count at `lead/trail` stripped needles; if count > 1 at a more-stripped level while exact was unique, emit risk. Policy: `off|warn|refuse`.

### 9.4 Idempotent detection

Per update file: if emit equals current base → today's `PATCH_NO_EFFECT`. Under `--idempotent`, if old-side cannot be found but new-side uniquely exists as if already applied, succeed with `already_applied`.

### 9.5 Backend rule

Still no `diffy`/`flickzeug` apply on V4A text. `similar` observational / plan diffs only.

---

## 10. Error Handling

### 10.1 Philosophy

Fail closed; prefer repairable diagnostics over silent success.

### 10.2 New / extended codes

| Code | Exit | When |
| --- | --- | --- |
| `HASH_PIN_MISMATCH` | 5 | Pin ≠ snapshot |
| `VERIFY_FAILED` | 1 | Verify command non-zero; tree unchanged |
| `RISK_REFUSED` | 1 | Risk gate refuse |
| `RECEIPT_INVALID` | 2 | Bad receipt |
| `REVERT_STALE` | 5 | Tree ≠ receipt after hashes |
| `ALREADY_APPLIED` | 0 | Only as success marker field / optional code under idempotent |

Exact code set frozen in Phase 0 with [docs/errors.md](docs/errors.md) update.

### 10.3 Oracle generation

On hunk failure:

1. Collect up to N candidate spans (cap for JSON size).
2. Excerpts ≤ M lines each.
3. If a unique match exists under `--fuzzy=strip` but not exact, optionally include a repair patch that adds context/anchor—not an auto apply.

---

## 11. Performance Targets

### 11.1 Baseline workload

- 10 files, ≤50 hunks, ≤2 MiB total file payload.
- Locate+emit < 50 ms typical on developer hardware (warm).
- Verify time dominated by user command (not gated).

### 11.2 Shadow cost

Shadow materialization of touched files only: < 100 ms for baseline without verify command.

### 11.3 Receipt size

Default cap: refuse to embed bytes beyond `max_file_bytes`; store hashes + paths always.

### 11.4 Gates

Criterion bench `apply_update` remains; add bench for locate with risk counting.

---

## 12. Instrumentation Plan

### 12.1 Timers

`parse`, `snapshot`, `locate`, `risk`, `emit`, `shadow`, `verify_cmd`, `commit`, `receipt`.

### 12.2 Counters

Candidates emitted, fuzzy level used, already_applied count, verify fail count.

### 12.3 Debug

`AGENT_PATCH_DEBUG=1` traces locate windows (no file bodies by default).

---

## 13. Security Model

Baseline: [docs/threat-model.md](docs/threat-model.md).

Additions:

1. Verify commands are **user-supplied** and run with the user's privileges—document footguns; no shell interpolation beyond `sh -c` if explicitly chosen (**Phase 0**: argv array vs shell string).
2. Shadow roots use restrictive temp permissions.
3. Repair patches never execute.
4. Doctor never downloads or self-updates binaries.

---

## 14. Test Plan

### 14.1 Unit

- Fuzzy unique: one/zero/many at each level.
- Risk gate: unique exact with stripped near-misses.
- Idempotent already-applied vs `PATCH_NO_EFFECT`.
- Hash pin mismatch before locate.
- Oracle candidate ordering stability.
- Receipt round-trip inverse.

### 14.2 Integration

- `--plan` zero writes (`CountingFs`).
- `--verify` success promotes; failure leaves root byte-identical.
- `--revert` after apply restores blake3.
- Concurrent modification during verify window.
- CLI: `doctor` exit codes.

### 14.3 Fixtures / dogfood

- Extend `fixtures/dogfood` scenarios for verify/oracle/revert.
- Keep Codex subset green.
- `scripts/dogfood` covers new gates or a `scripts/dogfood-next` until stable.

### 14.4 Fuzz

Existing `parse_patch` / `path_policy` / `apply_update`; add fuzz for fuzzy normalizer not panicking.

### 14.5 Property

Locate→emit→locate idempotence under `--idempotent` for already-applied cases.

---

## 15. Repository Structure

```text
apply_patch/
├── IMPLEMENTATION_PLAN.md          # this plan
├── README.md
├── AGENTS.md
├── .envrc.example
├── .cursor/skills/agent-patch/
├── crates/agent-patch/
│   ├── src/engine/{locate,emit,matcher,apply,diff_summary}.rs
│   ├── src/{commit,plan,shadow,receipt,verify,doctor,…}.rs
│   ├── tests/
│   ├── tests/fixtures/codex-scenarios/
│   └── benches/
├── fixtures/dogfood/               # manual / agent dogfood tree
├── fuzz/
├── scripts/{agent-patch,dogfood,test,lint,bench}
└── docs/
    ├── contract-v1.md
    ├── protocol.md
    ├── errors.md
    ├── design/
    ├── archive/2026-07-greenfield-implementation-plan.md
    └── research-*.md
```

---

## 16. Parallel Agent Work Plan

### 16.1 Workstream boundaries

| Agent | Owns | Must not touch |
| --- | --- | --- |
| A — Contract/docs | protocol, errors, schemas | engine internals |
| B — Oracle diagnostics | error JSON, excerpts, repair_patch | commit |
| C — Fuzzy + risk | matcher/locate policies | FS |
| D — Plan CLI | `--plan` payload | verify |
| E — Shadow + verify | shadow FS, verify runner | parser grammar |
| F — Receipt + revert | receipt serde, inverse plan | fuzzy |
| G — Doctor + PATH DX | doctor, README/skill | apply semantics |
| H — Dogfood/CI | fixtures, dogfood script, CI | contract freeze |

### 16.2 Freeze first

1. Flag matrix and mutual exclusions.
2. Error code list + JSON extensions.
3. Receipt schema + size policy.
4. Shadow strategy A vs B.
5. Verify command execution model (argv vs shell).
6. Fuzzy levels shipped in first bump.
7. Whether JSON `version` stays 1.

### 16.3 Integration order

```text
docs freeze → oracle errors → --plan → fuzzy/risk/idempotent
  → receipts/revert → shadow/verify → doctor → dogfood/CI
```

Optional parallel: `translate` after `--plan` diffs exist.

---

## 17. Implementation Phases and Acceptance Criteria

### Phase 0 — Contract freeze

Deliver:

- matching/CLI/error/receipt/shadow decisions written into docs;
- open decisions (§20) closed or explicitly deferred.

Acceptance:

- no ambiguous flag semantics;
- parallel agents can implement without guessing.

### Phase 1 — Oracle diagnostics

Deliver:

- candidates + excerpts on hunk failures;
- optional repair_patch when fuzzy-unique exists;
- docs + tests.

Acceptance:

- ambiguous fixture returns ≥2 candidates;
- JSON size capped;
- no auto-apply of repair.

### Phase 2 — `--plan`

Deliver:

- structured plan + per-file unified or line diffs via `similar`;
- zero writes.

Acceptance:

- parity with `--check` failure modes;
- CountingFs shows no mutating calls.

### Phase 3 — Fuzzy unique + risk + idempotent

Deliver:

- `--fuzzy`, `--risk`, `--idempotent`;
- contract bump;
- unit + dogfood coverage.

Acceptance:

- never first-match;
- risk refuse blocks commit;
- idempotent replay exits 0 with `already_applied`.

### Phase 4 — Hash pins

Deliver:

- parse + validate pins;
- `HASH_PIN_MISMATCH`.

Acceptance:

- pin fail before locate;
- Codex-style fixtures still pass without pins.

### Phase 5 — Receipts + revert

Deliver:

- `--receipt`, `--revert`;
- bounded storage policy.

Acceptance:

- apply→revert restores blake3 for add/update/delete sets;
- stale revert fails closed.

### Phase 6 — Shadow verify

Deliver:

- `--verify`;
- shadow strategy A;
- cleanup.

Acceptance:

- failing verify leaves root unchanged;
- passing verify matches direct apply bytes;
- path escape still rejected.

### Phase 7 — Doctor + DX

Deliver:

- `doctor` subcommand;
- README/skill/AGENTS updates;
- stale release detection.

Acceptance:

- doctor fails on obvious stale release when sources newer;
- documents PATH/direnv/`scripts/agent-patch`.

### Phase 8 — Hardening

Deliver:

- extended dogfood;
- fuzz/bench updates;
- optional Move (separate contract) or explicit defer;
- optional `translate`.

Acceptance:

- CI green Linux/macOS;
- performance non-regressions on apply bench;
- research-next-pass updated to current backlog only.

---

## 18. Verification Commands

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
scripts/dogfood
scripts/bench
cargo +nightly fuzz run parse_patch -- -max_total_time=30

# planned
scripts/agent-patch doctor
scripts/agent-patch --plan --json < change.patch
scripts/agent-patch --verify 'cargo check -q' < change.patch
scripts/agent-patch --receipt /tmp/r.json < change.patch
scripts/agent-patch --revert /tmp/r.json
```

Fixture dogfood:

```bash
agent-patch --root fixtures/dogfood --check < fixtures/dogfood/some.patch
```

---

## 19. Claude Code and Agent Instructions

Operational source of truth: [AGENTS.md](AGENTS.md), [`.cursor/skills/agent-patch/SKILL.md`](.cursor/skills/agent-patch/SKILL.md).

Durable rules for agents implementing this plan:

1. Prefer `scripts/agent-patch` / bare `agent-patch` for multi-hunk edits; dogfood on `fixtures/dogfood` before touching crate sources when experimenting.
2. Contract bump before matching/success-mode changes.
3. No whole-file rewrite recovery after `HUNK_*`.
4. No nesting heredocs inside `$(...)`.
5. Rebuild release after engine changes before trusting the wrapper (`doctor` / `cargo build --release`).
6. Keep docs in current-state voice.

---

## 20. Open Decisions to Resolve Before Coding

Phase 0 must decide:

1. JSON schema version: stay `1` additive vs bump to `2`.
2. Shadow strategy A (touched paths only) vs B (worktree/overlay).
3. Verify execution: argv list vs `sh -c` string; working directory and env vars.
4. Receipt storage: hashes-only vs bounded base64 bodies; default path.
5. Fuzzy levels in first bump: `rstrip` only vs `rstrip+strip`.
6. Risk default: `off` vs `warn`.
7. Idempotent default: off vs on.
8. Whether `ALREADY_APPLIED` is a separate exit/code or only a success field.
9. Hash pin header grammar and multi-hash algorithms (BLAKE3-only recommended).
10. Whether `--verify` implies receipt auto-write.
11. Mutual exclusion: `--verify` with `--check` / `--plan`.
12. Move File: implement in this plan’s Phase 8 or keep deferred.
13. `translate` in-scope or backlog-only.
14. Maximum candidates/excerpts bytes in error JSON.
15. Doctor severity: warn vs non-zero exit on stale release.

Recommended defaults:

```text
json version              1 additive fields
shadow                    A (touched paths) + docs for limits
verify                    argv array; cwd = shadow root
receipt                   hashes + paths; optional bodies ≤ max_file_bytes
fuzzy first ship          rstrip + strip; default off
risk default              warn in human, refuse in --json CI profiles later; ship default off
idempotent default        off
already_applied           success field; exit 0
hash pins                 blake3 only
verify ≠ check            exclusive
Move                      deferred (design/move.md)
translate                 backlog after --plan
oracle caps               ≤8 candidates; ≤20 lines/excerpt; ≤16KiB repair_patch
doctor stale release      exit 1 when release older than src and release is what wrapper selects
```

---

## 21. Definition of Done

This plan is complete when:

1. Phase 0 decisions are recorded in contract/protocol/errors docs.
2. Oracle diagnostics ship and are covered by tests.
3. `--plan` provides structured preview with zero writes.
4. Unique-only `--fuzzy` and risk/idempotent modes match the bumped contract.
5. Hash pins fail closed before locate.
6. Receipts + revert restore content for supported ops.
7. `--verify` promotes only on command success and never leaves partial real-root mutation on verify failure.
8. `doctor` detects PATH/direnv/stale-release issues.
9. Linux and macOS CI pass (`fmt`, `clippy -D warnings`, `test`, `dogfood`).
10. Fuzz targets remain crash-free for agreed smoke durations.
11. README, AGENTS, and `.cursor/skills/agent-patch` describe current behavior in present tense.
12. No silent fuzzy first-match, no whole-file fallback, no `diffy`/`flickzeug` V4A apply path.
13. `fixtures/dogfood` exercises verify/oracle/revert or documents temporary gaps.
14. Archived greenfield plan remains historical only; this file is the active plan.
15. A new agent can: read §3 and §17, implement one phase, and validate with §18 within one working session.
