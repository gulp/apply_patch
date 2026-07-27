# `agent-patch` — Implementation Plan

Status: Active post-v1 plan (agent reliability, recovery, and verification)
Supersedes: [docs/archive/2026-07-greenfield-implementation-plan.md](docs/archive/2026-07-greenfield-implementation-plan.md)
Authoritative behavior today: [README.md](README.md), [docs/contract-v1.md](docs/contract-v1.md), [docs/protocol.md](docs/protocol.md), [docs/design/](docs/design/)
Primary users: Coding agents operating through shell-capable harnesses
Primary interface: Repo-local command-line executable (`scripts/agent-patch`; optional PATH via direnv / `cargo install`)
Implementation language: Rust
Initial platforms: Linux and macOS
Primary objective: Keep fail-closed V4A apply while making verification representative, commits crash-recoverable, reverts self-contained, and multi-agent races explicit

---

## 1. Goals / Non-goals

### 1.1 Goals

1. Preserve the v1 contract: unique-exact locate→emit, transactional commit, root confinement, stable exits/JSON, no silent fuzzy default.

2. Make every apply failure **actionable for an agent** without human intervention:
   - candidate locations and excerpts derived from locator evidence;
   - draft repair patches where safe;
   - explicit next-action hints tied to `ErrorCode`.

3. Support **verify-gated commit**: materialize a representative shadow workspace, run a bounded user command, promote only on success and unchanged bases.

4. Emit **self-contained apply receipts** (content-addressed before-images) and support transactional **revert** without Git.

5. Make every mutating operation **crash-recoverable** via a durable journal and `recover`.

6. Support optional **content-hash pins** on patch targets so parallel agents fail closed on stale bases before hunk matching.

7. Support **idempotent / already-applied** detection as an explicit success mode that proves the full intended after-state (not silent no-op confusion with `PATCH_NO_EFFECT`).

8. Offer **unique-only** opt-in fuzz (`rstrip` / `strip`) that never selects first-match-wins.

9. Detect **wrong-match risk** from structured `MatchEvidence` and refuse or warn before commit.

10. Provide structured **`--plan`** output (immutable `ExecutionPlan` + diffs) without writing the tree.

11. Bound matcher work, shadow disk/time, verifier runtime/descendants/output, diagnostics, and recovery artifacts (exit 7 on limit failures whenever possible before mutation).

12. Ship **`doctor`** / **`status`** for PATH, direnv, release-binary freshness, lock/journal health, and artifact cleanup guidance.

13. Keep invocation agent-friendly:
    - `scripts/agent-patch` canonical;
    - bare `agent-patch` when `scripts/` is on `PATH`;
    - no MCP requirement;
    - no remote telemetry requirement.

14. Extend the dialect only via explicit contract bumps (`docs/contract-v1.md` or a successor).

15. Keep Move File and `translate` out of this plan’s critical path (backlog only; Move design remains [docs/design/move.md](docs/design/move.md)).

16. Keep dogfood fixtures and Codex scenario subsets green on Linux and macOS CI.

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

11. Not a remote telemetry agent, daemon, or bundled alerting service.

12. Not a sandbox for untrusted verifier commands; verification runs with the invoking user’s privileges.

13. Not Move or `translate` until recovery, self-contained receipts, and representative verification meet Definition of Done.

---

## 2. Product Definition

### 2.1 Project

`agent-patch`

A repo-local Rust CLI that applies structured, localized, transactional V4A-family patches for coding agents—with verification, recoverable failures, crash-safe commits, and multi-agent freshness controls.

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
- parallel agents race without content pins or writer serialization;
- mid-commit crashes leave an ambiguous tree with only in-process rollback;
- receipts that store hashes alone cannot reconstruct deleted or overwritten bytes;
- touched-path-only verify shadows create false verification claims;
- stale `target/release` binaries silently run old code.

This plan addresses those gaps without abandoning fail-closed semantics.

### 2.4 Constraints

- Contract-first: bump docs before behavior.
- Pure engine: locate/emit stay FS-free.
- One immutable `ExecutionPlan` drives verify, commit, receipt, revert, and recovery—no commit-time rematching.
- Commit guarantee is durable recoverability to proven all-before or all-after (not multi-file atomic visibility).
- JSON remains versioned and machine-pure.
- Complexity budget: prefer features that reuse `locate_chunks`, `emit_chunks`, path policy, and `PublicError`; do not invent a second matching algorithm for diagnostics.
- Local-only operational evidence (`status` / `doctor` / optional event log)—no network telemetry.

### 2.5 Environment

- Linux and macOS (CI matrix).
- Rust stable workspace under `crates/agent-patch`.
- Invocation: `scripts/agent-patch` (release → debug → `cargo run`); optional `PATH_add scripts` via `.envrc`.
- Dogfood tree: `fixtures/dogfood` (never required for production use).
- On-disk tool state under `.agent-patch/` (objects, transactions, lock, optional events)—never part of the patch dialect.

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
| `--plan` | Freeze and emit the immutable execution plan + structured diffs; no writes | Additive CLI |
| `--verify -- <PROGRAM> [ARG ...]` | Representative shadow, overlay plan, run bounded argv command, promote on exit 0 | Additive; shadow contract |
| `--verify-shell <SCRIPT>` | Explicit shell escape hatch | Additive; security warning |
| `--verify-timeout <DURATION>` | Wall-clock deadline including descendant termination | Additive |
| `--verify-output-limit <BYTES>` | Per-stream capture cap; artifact retained | Additive |
| `--shadow-mode <tree\|touched>` | `tree` default (representative); `touched` labeled non-representative | Additive policy |
| `--receipt <PATH>` | Copy/export durable receipt after success (internal receipt always written) | Additive |
| `--revert <RECEIPT>` | Transactional undo from receipt | Additive |
| `recover [--transaction <ID>]` | Resolve crash-interrupted transaction journal | Additive |
| `status` | Report lock/journal/object-store health without mutation | Additive |
| `gc [--dry-run]` | Reference-safe object/artifact cleanup | Additive |
| `--fuzzy <off\|rstrip\|strip>` | Unique-only fuzz ladder; default `off` | Contract bump for matching |
| `--risk <off\|warn\|refuse>` | Deterministic gate over `MatchEvidence` | Additive policy |
| `--idempotent` | Prove full intended after-state; reject incompatible partial replay | Contract bump for success modes |
| `doctor` | Env, binary freshness, transaction health, cleanup guidance | Additive |

`Move File` and `translate` are backlog-only and are not part of this plan’s implementation phases.

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

Exact grammar frozen in Phase 0 (§20).

### 3.4 Matching contract (baseline + deltas)

Baseline: [docs/contract-v1.md](docs/contract-v1.md).

Deltas under bump:

1. `--fuzzy=rstrip|strip`: normalize for search only; accept only if **exactly one** match.
2. Locator emits `MatchEvidence` (levels tried, retained context, candidate counts, anchor/EOF, nearby twins). Risk policy is a pure function over that evidence; similarity never selects a target.
3. `--idempotent`: succeed only when every operation is newly applicable or provably already applied **and** the combined final tree equals the plan’s intended after-state. Incompatible partial replay → `PARTIALLY_APPLIED` (exit 1).

### 3.5 Stdout / stderr

Unchanged philosophy:

- human mode: summary stdout, diagnostics stderr;
- `--json`: one object stdout; stderr empty for structured failures;
- verifier captured output never contaminates `--json` stdout (artifacts + bounded tails only).

### 3.6 Exit taxonomy

Keep exits 0–7. Planned additive meanings stay within classes:

| Code | Class | Notes |
| --- | --- | --- |
| 0 | success | includes idempotent already-applied when enabled |
| 1 | does not apply | hunk / risk-refuse / partially-applied / verify failed|timeout|signalled after clean shadow discard |
| 2 | malformed / unsupported | including invalid receipt schema |
| 3 | I/O | |
| 4 | path policy | |
| 5 | concurrent / lock / hash pin mismatch | |
| 6 | internal / rollback / recovery required / ambiguous recovery | |
| 7 | limits | match work, shadow, verify output, artifact budgets |

---

## 4. Architecture

### 4.1 Layered architecture

```text
CLI / output adapters
  → Application command dispatcher
      → Parse + policy validation
      → Snapshot loader
      → Pure planner
          → locate / emit / risk / idempotence
          → immutable ExecutionPlan + plan_digest
      → read-only exits: check / plan
      → verification coordinator
          → representative WorkspaceSnapshot
          → overlay ExecutionPlan
          → bounded VerifyRunner
      → mutation coordinator
          → root lock
          → final revalidation
          → object store + durable journal
          → commit state machine
          → receipt finalization
      → recovery / revert / gc services
  → human / JSON / optional JSONL-event renderers
```

### 4.2 Core domain objects

```rust
struct ExecutionPlan {
    version: u32,
    root_identity: RootIdentity,
    entries: Vec<PlannedEntry>,
    match_evidence: Vec<MatchEvidence>,
    limits_used: LimitUsage,
    digest: PlanDigest,
}

struct PlannedEntry {
    path: RepoPath,
    operation: OperationKind,
    before: FileIdentity,
    after: FileIdentity,
    after_content: PlannedContent,
    permissions: PermissionPlan,
}

struct TransactionJournal {
    transaction_id: TransactionId,
    plan_digest: PlanDigest,
    state: JournalState,
    entries: Vec<JournalEntry>,
    created_at: Timestamp,
}
```

Canonical plan encoding is specified byte-for-byte. Maps are forbidden in digest-bearing structures unless key ordering is canonical. Paths are sorted by normalized repository-relative bytes.

### 4.3 Components

#### 4.3.1 CLI and application dispatcher

Parses modes and delegates to typed commands. It does not parse patch syntax, match hunks, or mutate files. Conflicts are rejected by a frozen flag matrix. Verify argv begins after `--`; no implicit shell parsing occurs.

#### 4.3.2 Protocol parser

Produces a source-spanned AST and optional BLAKE3 pins. Filesystem-free; bounded by input limits.

#### 4.3.3 Path policy and snapshot loader

Centralizes root confinement, path alias detection, symlink policy, special-file rejection, byte loading, metadata capture, and exact content identity. Every later layer receives `RepoPath`, never untrusted `PathBuf`.

#### 4.3.4 Pure planner

Owns validation, matching, emission, risk evaluation, and idempotence proofs. Returns one immutable `ExecutionPlan`; no downstream phase rematches or reconstructs intended bytes.

#### 4.3.5 Match evidence and risk policy

Locator records every attempted exact / context-reduced / fuzzy search and its candidate count. Diagnostics and risk consume this evidence. Repair patches may be rendered from evidence but are never executable state.

#### 4.3.6 Workspace snapshotter

Creates an isolated candidate workspace from the current root, then overlays the plan:

- includes tracked, dirty, and untracked files under policy;
- preserves bytes, executable bits, safe in-root symlinks, and directory shape;
- uses reflink/clonefile when available and verified not to mutate the source;
- falls back to byte copy;
- never uses hard links;
- always excludes `.agent-patch/` internals (recreated minimally in the shadow);
- by default also excludes VCS metadata and common build/cache trees (`.git/`, `target/`, `node_modules/`, `.venv/`, `__pycache__/`, and equivalents) so verify remains usable on large checkouts—JSON reports `shadow.excludes` and still sets `verify.representative=true` when the retained tree plus plan is a coherent source workspace for typical build/test commands;
- `--shadow-include-caches` (or empty exclude list) opts into near-byte-complete trees when budgets allow;
- records a manifest and total files/bytes;
- fails before verify if shadow budgets are exceeded.

`--shadow-mode=touched` materializes only planned paths, sets `verify.representative=false`, and is never the default.

#### 4.3.7 Verify runner

Runs argv directly with `cwd` equal to the shadow root. Creates a process group, streams bounded output to artifact files, enforces timeout, graceful then hard kill, waits for descendants, and records exit/signal/duration/truncation. Environment additions:

```text
AGENT_PATCH_MODE=verify
AGENT_PATCH_REAL_ROOT=<absolute root>
AGENT_PATCH_SHADOW_ROOT=<absolute shadow>
AGENT_PATCH_PLAN_DIGEST=<hex>
AGENT_PATCH_INVOCATION_ID=<id>
```

#### 4.3.8 Root lock

Mutating apply / revert / recover acquire an exclusive lock at `.agent-patch/lock`. Acquisition has a bounded timeout and reports owner metadata when available. Stale-lock heuristics never delete transaction journals. `--check` / `--plan` remain lock-free. `--verify` locks only for final promotion after a successful verifier, then revalidates every affected path. The lock is defense in depth—never a replacement for byte-identity checks.

#### 4.3.9 Content-addressed object store

Stores exact before-images under `.agent-patch/objects/<blake3>`. Objects are immutable, exclusively created, hash-verified after write, and directory-fsynced. Written before visible mutation. Adds need no before-image; updates and deletes do. GC is explicit, reference-safe, dry-run capable, and never removes objects referenced by receipts or incomplete journals.

#### 4.3.10 Transaction coordinator

Consumes only an `ExecutionPlan`. Acquires the lock, rejects unresolved journals, revalidates bases, prepares temps and before-image objects, writes the journal, advances the commit state machine, verifies postconditions, finalizes the receipt, and marks the journal complete.

#### 4.3.11 Recovery service

Reads durable journal state without trusting filenames alone. Verifies hashes of journals, objects, temps, and current paths. Policy:

1. mark complete when every after-state already matches;
2. otherwise restore every before-state from objects;
3. never invent a mixed roll-forward;
4. retain evidence and exit 6 if neither complete nor rollback can be proven.

#### 4.3.12 Receipt and revert service

Receipts reference immutable before-image objects and include root identity, transaction ID, plan digest, after identities, permissions, and tool contract version. Revert proves current paths equal receipt after-states, then creates a new ordinary journaled transaction restoring before-states. Revert is itself receipted and recoverable.

#### 4.3.13 Diagnostics and observability

Public errors, success JSON, `status`, and `doctor` render from typed records. Optional JSONL event log (`AGENT_PATCH_EVENT_LOG=1` or `--event-log`) is an adapter—never required for correctness. Source excerpts, verifier tails, and repair patches have independent byte caps.

#### 4.3.14 Doctor

Checks selected binary freshness vs sources, PATH/direnv resolution, lock/journal health, corrupt/missing objects, leftover shadows, and unsupported durability capabilities.

### 4.4 Boundary rules

1. Parser, matcher, emitter, risk policy, and planner are pure.
2. Only the workspace snapshotter may create verification trees.
3. Only the verify runner may spawn commands.
4. Only the transaction coordinator and recovery service may mutate the real root or `.agent-patch/transactions`.
5. Commit never rematches hunks or changes the plan.
6. Receipts are finalized only after postcondition verification.
7. Repair patches are diagnostic strings, never auto-applied.
8. Hard links are forbidden in shadows and rollback storage.
9. A mutating command refuses to proceed while recovery is required.
10. Optional output adapters cannot alter exit codes or transaction behavior.

---

## 5. Core Invariants

Carry forward v1 invariants I1–I18 (root confinement, transactionality, no mutation before validation, no silent fallback, unique matching, …) from the archived plan and [docs/design/overview.md](docs/design/overview.md).

### I19 — Verify before promote

`--verify` never mutates the real root unless the verify command exits 0 and revalidation still passes. Verify failure/timeout/signal discards the shadow and leaves the root unchanged.

### I20 — Receipt fidelity

Revert restores exact before bytes and modes from durable objects referenced by a successful receipt, or fails closed with no partial undo.

### I21 — Fuzzy uniqueness

Any fuzz level still requires exactly one match; zero or many → existing hunk errors.

### I22 — Oracle honesty

Candidate lists and repair patches must be derived from the same locator evidence used for apply; no second guessed algorithm.

### I23 — Hash pin precedence

When a pin is present, pin failure precedes hunk matching.

### I24 — Mode exclusivity

`--check`, `--plan`, `--verify`, apply, and `--revert` do not silently combine conflicting write behaviors.

### I25 — One immutable plan

Every apply phase consumes the same digest-bearing `ExecutionPlan`; no commit-time rematching or intent reconstruction.

### I26 — Durable recoverability before visibility

Before the first visible root mutation, all required before-images and the transaction journal are durable and hash-verified.

### I27 — Single root writer

At most one apply, revert, or recover transaction owns the root mutation lock.

### I28 — Representative verify by default

Default `--verify` observes an isolated workspace that includes dirty/untracked source files and untouched dependencies needed for typical repository commands, subject to documented cache excludes (§4.3.6). Weaker modes (`touched`, or explicit exclude overrides that omit required sources) are machine-labeled `representative=false`.

### I29 — Bounded subprocess lifecycle

Verify commands have bounded wall time, output, termination grace, and descendant lifetime.

### I30 — Self-contained revert

A receipt references durable content sufficient to restore exact before-states without Git, network, or mutable external caches.

### I31 — No unresolved-journal bypass

Normal mutation cannot proceed while an incomplete transaction journal exists.

### I32 — Idempotence proves final state

Idempotent success means the complete intended after-state is proven; incompatible partial application is not success.

---

## 6. Data Flow

### 6.1 Plan / check

```text
input → bounded read → parse → path policy → snapshot
  → pins → locate/emit → MatchEvidence → risk/idempotence
  → freeze ExecutionPlan + digest → render → stop
```

No lock, temp file, shadow, object, journal, or real-root write is permitted.

### 6.2 Direct apply

```text
freeze ExecutionPlan
  → acquire root lock
  → reject unresolved journal
  → revalidate every base identity
  → persist before-image objects
  → prepare same-directory temps
  → write+fsync journal(PREPARED)
  → commit entries, journaling progress
  → verify every after identity
  → write+fsync receipt
  → journal(COMPLETED)
  → cleanup temps → release lock → emit success
```

### 6.3 Verify-gated apply

```text
freeze ExecutionPlan
  → snapshot representative workspace
  → overlay exact planned after-state
  → run bounded verifier
  → nonzero/timeout/signal: retain bounded artifacts, discard shadow, no root lock/write
  → success: acquire lock → reject unresolved journal → revalidate bases
  → execute the same direct-apply transaction protocol
```

The verifier result is tied to `plan_digest`. Root drift before promotion → `CONCURRENT_MODIFICATION`; verification is not silently rerun.

### 6.4 Revert

```text
load+validate receipt → resolve+verify object hashes
  → prove current paths equal receipt after-states
  → build inverse ExecutionPlan
  → ordinary journaled transaction → new revert receipt
```

### 6.5 Recover

```text
acquire root lock → load incomplete journal → verify durable artifacts/current paths
  → all after-states match: finalize/mark complete
  → otherwise: restore all before-states from objects
  → verify restored identities → mark rolled_back
  → ambiguity or missing artifacts: retain journal, exit 6, emit exact next-action evidence
```

### 6.6 Failure

No real-root mutation on validation / locate / risk / verify failure. Mid-commit failure uses in-process rollback when possible; otherwise leaves a recoverable journal. Every failure states whether the root changed, whether rollback completed, whether recovery is required, and the exact next action.

---

## 7. Data Model and Schemas

### 7.1 Success JSON

Public CLI JSON bumps to version `2` when `ExecutionPlan` / transaction fields ship (consumers must opt in). Shape:

```json
{
  "version": 2,
  "ok": true,
  "invocation_id": "01J...",
  "mode": "apply|check|plan|verify|revert|recover|status|doctor",
  "plan_digest": "blake3:...",
  "already_applied": false,
  "summary": {},
  "files": [],
  "plan": null,
  "receipt_path": null,
  "transaction_id": null,
  "verify": {
    "argv": ["cargo", "check", "-q"],
    "exit_code": 0,
    "duration_ms": 0,
    "representative": true,
    "shadow_mode": "tree",
    "excludes": [".git/", "target/"],
    "artifact_dir": null
  }
}
```

### 7.2 Error JSON

```json
{
  "ok": false,
  "invocation_id": "01J...",
  "error": {
    "code": "HUNK_AMBIGUOUS",
    "exit_code": 1,
    "message": "...",
    "path": "...",
    "root_changed": false,
    "recovery_required": false,
    "candidates": [{ "start_line": 10, "end_line": 12, "excerpt": "..." }],
    "repair_patch": "*** Begin Patch\n...",
    "hint": "..."
  }
}
```

### 7.3 Execution plan schema

`--plan` emits the digest-bearing plan: normalized paths, operation order, before/after identities, newline metadata, match evidence summaries, risk decisions, and structured diffs (`similar` observational only). Encoding rules for the digest live in the schema freeze (Phase 0).

### 7.4 Receipt schema

```json
{
  "version": 2,
  "root": "...",
  "created_at": "...",
  "transaction_id": "...",
  "plan_digest": "blake3:...",
  "tool_contract_version": 2,
  "files": [
    {
      "path": "src/a.rs",
      "operation": "update|add|delete",
      "before_blake3": "...",
      "after_blake3": "...",
      "before_object": "objects/<blake3>",
      "permissions": { "executable": false }
    }
  ]
}
```

Internal receipts always live under `.agent-patch/receipts/`. `--receipt <PATH>` exports a copy. Before bytes live in the object store—not inline base64—except optional small diagnostic embeds under a hard cap.

### 7.5 Journal schema and states

Journal states (minimum): `PREPARED` → `COMMITTING` → `COMPLETED` | `ROLLING_BACK` → `ROLLED_BACK`. Progress records per-entry completion. Incomplete journals block new mutation until `recover` resolves them.

### 7.6 Object schema

Objects are exact byte blobs addressed by BLAKE3. Restore metadata lives in receipt/journal entries, not in filesystem mtimes. Write protocol: exclusive create → full write → file sync → hash reread → atomic publish → directory sync.

### 7.7 Fingerprints and newlines

BLAKE3 over exact bytes is authoritative. Existence, kind, size, and executable mode are recorded for diagnostics and race defense. LF/CRLF file-wins; mixed line endings remain rejected on update; BOM preserve unchanged.

---

## 8. Filesystem Transaction Strategy

### 8.1 Filesystem support contract

Supported roots must provide same-directory atomic rename for regular files. Unsupported special files and cross-device temp placement are rejected. Multi-file atomic visibility is not claimed. The guarantee is durable recoverability to a proven all-before or all-after state. See also [docs/design/transaction.md](docs/design/transaction.md) (updated in Phase 0/3 to journal + objects).

### 8.2 Preparation

Under the root lock and after final revalidation:

1. create transaction directory exclusively;
2. store and verify before-image objects for updates/deletes;
3. create same-directory temp files for adds/updates;
4. write proposed bytes, set permissions, sync files;
5. record all temp paths and identities;
6. write and fsync `PREPARED` journal;
7. fsync every relevant parent directory before visible mutation.

No delete or rename occurs before step 6 completes.

### 8.3 Commit linearization and ordering

The first successful visible rename/delete is the transaction linearization point. Entries are ordered lexicographically by normalized path. After each visible step, journal progress is durably advanced. Adds use no-overwrite primitives where available and fail closed on collisions.

### 8.4 Postcondition verification

After all visible operations, every target is re-read and compared with the plan after-identity. Mismatch enters rollback/recovery; it is never reported as success. Receipt finalization follows successful postcondition verification.

### 8.5 In-process rollback

Rollback restores updates/deletes from immutable objects and removes adds, journaling each step. Success requires exact before identities. Failure returns `ROLLBACK_FAILED`, keeps the journal and artifacts, sets `recovery_required=true`, and exits 6.

### 8.6 Crash recovery

On startup of any mutating command, incomplete journals are detected before planning commit. The command exits with `RECOVERY_REQUIRED` and a precise `agent-patch recover` invocation. Recovery never relies on process IDs or stale lock deletion alone.

### 8.7 Workspace snapshot strategy

Default `tree` mode walks the root once under path policy, default excludes (§4.3.6), and budgets. Reflink/clonefile only when clone writes cannot alter the source. Hard links prohibited. Symlinks copied as symlinks only when lexical and resolved targets remain within root; otherwise verify fails by policy.

### 8.8 Cleanup and garbage collection

Temporary shadows and verifier artifacts are removed on ordinary success unless retention is requested. Transaction artifacts are removed only after journal completion and receipt durability. Object GC is mark-and-sweep over receipts plus incomplete journals, runs only through explicit `gc`, and supports dry-run.

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

For each accepted chunk, retain complete `MatchEvidence`: attempted levels, context retained, candidates at each level, anchor/EOF behavior, and nearby twins. `off|warn|refuse` is a deterministic pure mapping over this record. Risk may refuse weak evidence but may never use similarity to select among candidates.

### 9.4 Idempotent detection

Without `--idempotent`, emit-equals-base remains `PATCH_NO_EFFECT` (failure). With `--idempotent`, evaluate per hunk, per operation, and for the complete patch:

- Adds already applied only when exact intended bytes and permissions exist;
- Deletes only when the path is absent;
- Updates only when the planner proves the exact intended after-state.

A mix of newly applicable and already-applied operations is allowed only when they compose to one unambiguous final state. Incompatible partial replay → `PARTIALLY_APPLIED`.

### 9.5 Backend rule

Still no `diffy`/`flickzeug` apply on V4A text. `similar` observational / plan diffs only.

---

## 10. Error Handling

### 10.1 Philosophy

Fail closed, preserve evidence, and distinguish semantic rejection from operational damage. Prefer repairable diagnostics over silent success.

### 10.2 Error classes

| Code | Exit | Retry guidance |
| --- | --- | --- |
| `HASH_PIN_MISMATCH` | 5 | reread / regenerate |
| `CONCURRENT_MODIFICATION` | 5 | reread / regenerate; verification is stale |
| `ROOT_LOCKED` | 5 | retry after owner exits; do not remove journal |
| `RECOVERY_REQUIRED` | 6 | run `recover` |
| `VERIFY_FAILED` | 1 | inspect bounded artifacts |
| `VERIFY_TIMEOUT` | 1 | change timeout / command |
| `VERIFY_SIGNALLED` | 1 | inspect signal / artifacts |
| `RISK_REFUSED` | 1 | add context / anchor |
| `PARTIALLY_APPLIED` | 1 | reread and regenerate complete patch |
| `RECEIPT_INVALID` | 2 | use valid supported receipt |
| `RECEIPT_OBJECT_MISSING` | 6 | restore objects / manual intervention |
| `REVERT_STALE` | 5 | do not overwrite subsequent edits |
| `SHADOW_LIMIT_EXCEEDED` | 7 | narrow mode or raise explicit limit |
| `MATCH_WORK_LIMIT` | 7 | add context / split patch |
| `ROLLBACK_FAILED` | 6 | run `recover`; preserve artifacts |
| `RECOVERY_AMBIGUOUS` | 6 | manual intervention with evidence |

`ALREADY_APPLIED` remains a success status field, not an error code. Exact catalog frozen in Phase 0 with [docs/errors.md](docs/errors.md).

### 10.3 Retry policy

Semantic, policy, concurrency, verify, and limit failures are never blindly retried. Internal retries are limited to interrupted syscalls where the operation is known not to have crossed a visible mutation boundary. Ambiguous filesystem completion is resolved by rereading postconditions.

### 10.4 Oracle generation

On hunk failure:

- at most 8 candidates;
- at most 20 lines per excerpt;
- all candidate/excerpt JSON combined ≤64 KiB;
- repair patch ≤16 KiB;
- deterministic ordering by path and line;
- candidates derived exclusively from locator evidence;
- no source body logged outside explicit diagnostics.

### 10.5 Verifier output handling

Stdout and stderr independently capped. JSON contains bounded tails and paths to retained artifacts. Broken pipes, invalid encoding, timeout, signal, and descendant-kill failure have distinct fields/codes.

### 10.6 Startup health gate

Mutating commands first validate `.agent-patch/` structure, lockability, journal versions, and referenced object availability. Health failure occurs before patch parsing can lead to mutation.

---

## 11. Performance and Scalability Targets

### 11.1 Direct plan/apply workload

Baseline: 10 files, ≤50 hunks, ≤2 MiB affected bytes. Warm-cache targets on developer/CI reference hardware:

- parse + snapshot + plan: typically < 50 ms;
- direct apply excluding durability sync variance: typically < 100 ms;
- no unrelated root file reads outside path-policy ancestors and `.agent-patch/` health checks.

### 11.2 Matching complexity

Exact/fuzzy candidate search must be linear or near-linear in file bytes per attempted needle. The planner tracks `match_work_units`; defaults cap total attempted byte comparisons. Pathological repetitive fixtures must terminate under a fixed bound or fail with `MATCH_WORK_LIMIT`.

### 11.3 Shadow scalability

Default budgets (configurable within hard maxima):

```text
max_shadow_files       200,000
max_shadow_bytes       20 GiB
max_shadow_wall_time   120 s
```

Snapshotter reports discovery/copy/reflink counts and fails before verifier launch when limits are exceeded. Default excludes (§4.3.6) keep typical Rust/JS repos under budget without requiring hard links.

### 11.4 Verify runner defaults

```text
verify_timeout         10 min
verify_kill_grace      5 s
verify_stdout_limit    8 MiB
verify_stderr_limit    8 MiB
```

### 11.5 Transaction artifacts

Before-image storage is deduplicated by hash. A patch that cannot be made recoverable within existing total affected-byte limits is rejected before mutation. Journal updates remain O(number of entries), not O(file bytes).

### 11.6 Performance gates

Criterion bench `apply_update` remains; add benches for locate+risk and shadow materialization. CI fails on clear regressions (plan/apply median) and on leaked shadows / surviving verifier children after kill bound.

---

## 12. Observability and Operational Robustness

### 12.1 Invocation correlation

Every run receives a sortable invocation ID. Mutating runs also receive a transaction ID. Both appear in human diagnostics, JSON, journals, receipts, and artifact paths.

### 12.2 Timers

`parse`, `snapshot`, `locate`, `risk`, `emit`, `plan_freeze`, `shadow`, `verify_cmd`, `lock_wait`, `revalidate`, `object_store`, `prepare`, `commit`, `postverify`, `rollback`, `receipt`, `cleanup`, `recover`.

### 12.3 Counters

Files/bytes/hunks, match work units, candidates, fuzz levels, risk findings, shadow copied/reflinked bytes, verifier output bytes, descendants killed, object bytes reused/new, journal transitions, unresolved transaction count.

### 12.4 Event log (optional)

With `AGENT_PATCH_EVENT_LOG=1` or `--event-log <PATH>`, emit versioned JSONL metadata records (no patch/file bodies by default). Event-log write failure is a warning before mutation unless configured as required. Optional adapters (e.g. CI annotations) must not change exit semantics.

### 12.5 Health and orchestrator contract

`status --json` and `doctor --json` expose checks with `ok|warn|error`:

- unresolved / incompatible journals;
- missing / corrupt referenced objects;
- held lock and owner metadata;
- stale selected binary;
- leftover shadows/artifacts past retention;
- object-store size and GC eligibility;
- unsupported filesystem semantics.

A health `error` exits non-zero. That is the alerting contract for orchestrators—not a monitoring daemon.

### 12.6 Debug

`AGENT_PATCH_DEBUG=1` traces locate windows (no file bodies by default).

---

## 13. Security Model

Baseline: [docs/threat-model.md](docs/threat-model.md).

Additions:

1. Verify commands are **user-supplied** and run with the user’s privileges. Canonical form is argv after `--`; shell requires explicit `--verify-shell`.
2. Shadow roots use restrictive temp permissions; hard links forbidden so verifiers cannot mutate the real tree through shared inodes.
3. Repair patches never execute.
4. Doctor never downloads or self-updates binaries.
5. `.agent-patch/` is root-confined tool state; path policy rejects escapes.
6. Event logs omit source bodies by default.

---

## 14. Test Plan

### 14.1 Unit

- Fuzzy unique: one/zero/many at each level.
- Risk gate: unique exact with stripped near-misses; evidence determinism.
- Idempotent already-applied vs `PATCH_NO_EFFECT` vs `PARTIALLY_APPLIED`.
- Hash pin mismatch before locate.
- Oracle candidate ordering stability and size caps.
- Plan digest stability under key reordering attempts.
- Object store write/verify/GC reference rules.
- Journal state transitions and recovery decision table.
- Verify runner timeout, signal, output truncation, process-group reaping.
- Receipt round-trip inverse (apply → exit process → revert).

### 14.2 Property and model-based

- Locate→emit→locate idempotence under `--idempotent` for already-applied cases.
- Model-based commit/recover state machine: every interrupted transition recovers to all-before or all-after.
- Differential: direct apply bytes == verify-promote bytes when root unchanged.

### 14.3 Filesystem fault-injection / crash E2E

Failpoint sweeps across journal/rename/fsync transitions. Killpoint matrix: SIGKILL during prepare, mid-rename, postcondition, receipt finalize. Incomplete journals block new writers until `recover`.

### 14.4 Integration / CLI

- `--plan` / `--check` zero writes (`CountingFs`).
- `--verify` success promotes; failure/timeout leaves root byte-identical.
- `--shadow-mode=touched` sets `representative=false`.
- Concurrent modification during verify window → `CONCURRENT_MODIFICATION`.
- `doctor` / `status` exit codes; `gc --dry-run`.

### 14.5 Fixtures / dogfood

- Extend `fixtures/dogfood` for verify/oracle/revert/recover.
- Keep Codex subset green.
- `scripts/dogfood` covers new gates or a `scripts/dogfood-next` until stable.

### 14.6 Cross-platform

Cover executable bits, clonefile/reflink fallback, directory fsync support, rename semantics, symlink policy, and explicit capability errors when guarantees are unsupported.

### 14.7 Fuzz

Existing `parse_patch` / `path_policy` / `apply_update`; add fuzz for fuzzy normalizer and plan encoding round-trips.

### 14.8 Schema

Golden JSON Schema validation for success/error/plan/receipt/journal fixtures; compatibility tests on version bumps.

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
│   ├── src/{commit,plan,shadow,receipt,verify,doctor,journal,objects,…}.rs
│   ├── tests/
│   ├── tests/fixtures/codex-scenarios/
│   └── benches/
├── fixtures/dogfood/
├── fuzz/
├── scripts/{agent-patch,dogfood,test,lint,bench}
└── docs/
    ├── contract-v1.md              # or contract-v2 when bumped
    ├── protocol.md
    ├── errors.md
    ├── design/
    ├── archive/2026-07-greenfield-implementation-plan.md
    └── research-*.md
```

On-disk runtime layout (not source):

```text
<root>/.agent-patch/
├── lock
├── objects/
├── transactions/<txid>/journal.json
├── receipts/
└── events/                         # optional
```

---

## 16. Parallel Agent Work Plan

### 16.1 Workstream boundaries

| Agent | Owns | Must not touch |
| --- | --- | --- |
| A — Contract/docs | protocol, errors, schemas, transaction design | engine internals |
| B — ExecutionPlan + oracle | plan freeze, evidence, error JSON, `--plan` | commit/journal |
| C — Fuzzy + risk + idempotent | matcher/locate policies | FS mutation |
| D — Object store + journal + lock | `.agent-patch/` layout, durability | parser grammar |
| E — Commit/recover | state machine, failpoints | fuzzy |
| F — Receipt + revert + GC | receipt serde, inverse plan | verify runner |
| G — Shadow + verify | snapshotter, verify runner, promote | parser grammar |
| H — Doctor/status/CI | health, dogfood, crash CI | apply semantics |

### 16.2 Freeze first (Phase 0)

1. JSON / plan / receipt / journal schema version 2 and canonical plan encoding.
2. Root identity and `.agent-patch/` layout.
3. Journal states, linearization point, recovery decision table.
4. Object-store durability and GC reference rules.
5. Lock semantics and timeout.
6. Tree-shadow inclusion/exclusion, symlink, and resource policy.
7. Verify argv, explicit shell, timeout, output, process-group contract.
8. Idempotence and partial-application truth table.
9. Match evidence and risk-policy truth table.
10. Error code and flag compatibility matrix.

### 16.3 Integration order

```text
schemas + recovery design
  → immutable plan + oracle + --plan
  → object store + journal + lock
  → journaled commit + recover
  → receipts + revert + gc
  → representative shadow
  → bounded verify + promote
  → fuzzy / risk / pins / idempotence
  → status / doctor / optional events / crash-soak CI
```

Move and `translate` remain outside this plan.

---

## 17. Implementation Phases and Acceptance Criteria

### Phase 0 — Contract and state-machine freeze

Deliver: schemas, canonical encodings, flag matrix, journal/recovery decision table, shadow exclude policy, verify runner contract, error taxonomy into docs.

Acceptance: independent agents can implement from fixtures without guessing; schema compatibility tests pass; §20 decisions recorded as frozen.

### Phase 1 — Immutable execution plan and oracle evidence

Deliver: digest-bearing `ExecutionPlan`, `MatchEvidence`, bounded diagnostics, `--plan`, `--check` parity.

Acceptance: plan digest is deterministic; `--check`/`--plan` make zero mutating calls; diagnostics derive from the same evidence; ambiguous fixture returns ≥2 candidates; no auto-apply of repair.

### Phase 2 — Object store, journal, and root lock

Deliver: `.agent-patch/` layout, CAS, journal state transitions, lock, health checks, failpoint hooks.

Acceptance: no visible mutation can occur before durable objects and `PREPARED` journal; corrupt/missing artifacts fail closed.

### Phase 3 — Journaled commit and recover

Deliver: commit state machine, rollback, startup recovery gate, `status`, `recover`.

Acceptance: killpoint sweep proves every interrupted transaction recovers to exact all-before or all-after; unresolved ambiguity remains blocked with evidence.

### Phase 4 — Receipts, revert, and GC

Deliver: canonical internal receipts, `--receipt` export, inverse plans, recoverable revert, reference-safe dry-run GC.

Acceptance: apply → process exit → revert restores exact bytes and modes without Git; stale revert fails before mutation; GC preserves closure.

### Phase 5 — Representative workspace shadow

Deliver: tree snapshot, manifest, reflink/clonefile acceleration, copy fallback, safe symlinks, default excludes, budgets, explicit touched mode.

Acceptance: dirty/untracked/untouched source dependencies are visible under default policy; shadow mutation cannot affect real root; limits fail before verifier launch; `touched` is labeled non-representative.

### Phase 6 — Bounded verify runner and promotion

Deliver: argv mode, explicit shell mode, timeout, output artifacts, process reaping, verify→promote revalidation.

Acceptance: failing/timeout/signalled verify never mutates root; passing verify bytes equal direct apply; grandchildren do not survive; root drift blocks promotion.

### Phase 7 — Fuzzy, risk, pins, and idempotence

Deliver: unique-only fuzz, deterministic risk gate, hash pins, full-state idempotence, partial replay rejection.

Acceptance: no first-match path exists; pin failure precedes matching; every idempotent success proves intended final tree.

### Phase 8 — Operational hardening

Deliver: `doctor` freshness checks, optional event logs, retention cleanup, crash/soak/performance CI, Linux/macOS capability docs, complete dogfood.

Acceptance: health errors are machine-actionable; performance gates pass; no leaked processes/shadows/temps; docs describe current behavior in present tense.

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
cargo test --workspace --features failpoints crash_matrix
cargo test --workspace verify_process_reaping
scripts/agent-patch doctor
scripts/agent-patch status --json
scripts/agent-patch --plan --json < change.patch
scripts/agent-patch --verify -- cargo check -q < change.patch
scripts/agent-patch --receipt /tmp/r.json < change.patch
scripts/agent-patch --revert /tmp/r.json
scripts/agent-patch recover --json
scripts/agent-patch gc --dry-run --json
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
7. Never bypass an unresolved journal or remove `.agent-patch/lock` / transactions manually.
8. Never use hard links to build a verification shadow.
9. Do not report success until after-state verification and receipt durability complete.
10. Keep verifier commands argv-based unless the user explicitly chooses shell mode.

---

## 20. Frozen Decisions

1. Public CLI JSON, plan, receipt, and journal schemas use version 2 when these features ship.
2. Default verify shadow is `tree` (representative under documented excludes); `touched` is explicit and labeled non-representative.
3. Default shadow excludes: `.agent-patch/`, `.git/`, and common build/cache trees (`target/`, `node_modules/`, `.venv/`, `__pycache__/`, equivalents). Near-complete trees require an explicit include-caches opt-in and must still respect budgets.
4. Verify executes argv after `--`; shell execution requires `--verify-shell`.
5. Verify defaults: 10-minute timeout, 5-second kill grace, 8 MiB per output stream.
6. Receipts reference self-contained immutable before-image objects; hashes-only receipts are rejected as non-recoverable.
7. `.agent-patch/transactions` journals are durable before visible mutation.
8. Apply / revert / recover serialize through a root lock and still revalidate exact content.
9. `ALREADY_APPLIED` is a success status; `PARTIALLY_APPLIED` is exit 1.
10. Hash pins are BLAKE3-only in the first contract bump.
11. Fuzzy ships with `rstrip` and `strip`, default off, always unique-only.
12. Risk default is off for compatibility; CI profiles may explicitly choose refuse.
13. `--verify` and read-only `--check` / `--plan` are mutually exclusive.
14. Successful verify does not auto-write a user-named receipt, but every mutation writes an internal canonical receipt.
15. Oracle caps: 8 candidates, 20 lines each, 64 KiB total candidate payload, 16 KiB repair patch.
16. Doctor / status return non-zero on unresolved journals, corrupt objects, or unsupported durability guarantees; stale binary is error only when it is the selected executable.
17. Move and `translate` are backlog-only until this plan’s Definition of Done.
18. Hard links are forbidden for shadows and transactional storage.
19. Object GC is explicit, reference-safe, and dry-run capable.
20. Optional JSONL event logging is an observability adapter, not a correctness dependency.

---

## 21. Definition of Done

This plan is complete when:

1. Contract docs and JSON Schemas for plan/receipt/journal are frozen and compatibility-tested.
2. One canonical `ExecutionPlan` and digest drive plan, verify, commit, receipt, revert, and recovery.
3. No visible mutation occurs before before-images and `PREPARED` journal are durable.
4. Killpoint tests recover interrupted transactions to exact all-before or all-after.
5. Mutating commands refuse to bypass unresolved journals and serialize through the root lock.
6. Internal receipts are self-contained through verified immutable objects; apply → restart → revert restores exact bytes and modes without Git.
7. Default verify observes a representative workspace (dirty/untracked sources + untouched dependencies under documented excludes); weaker modes are labeled.
8. Verify argv, timeout, output bounds, signals, and descendant cleanup are tested; no verifier process survives the kill bound.
9. Passing verify followed by unchanged-root promotion produces bytes identical to direct apply; root drift fails closed.
10. Unique-only fuzz, deterministic risk evidence, pins, and full-state idempotence match the bumped contract.
11. Plan/check remain write-free and parallel-safe.
12. `status` / `doctor`, journals, receipts, and artifacts provide enough evidence for an orchestrator to identify recovery-required and degraded states.
13. Linux and macOS CI pass formatting, lint, unit, property, integration, E2E, crash, process-leak, schema, dogfood, and agreed performance gates.
14. Resource budgets cover patch/file counts, match work, shadow files/bytes/time, verify time/output, diagnostics, objects, and artifacts.
15. No silent first-match, whole-file fallback, shell insertion, hard-link shadow, hashes-only revert, or unresolved-journal bypass exists.
16. Move and `translate` remain out of scope until this Definition of Done is met.
17. README, AGENTS, skill docs, protocol, error catalog, recovery notes, and threat model describe shipped behavior in present tense.
18. A new agent can implement any phase from its schemas, invariants, acceptance tests, and verification commands without inventing semantics.
19. Archived greenfield plan remains historical only; this file is the active plan.
