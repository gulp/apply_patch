# agent-patch Contract v2 (post-v1)

Frozen decisions for post-v1 features. v1 matching/ops remain unless explicitly superseded. Changing any field requires an explicit contract bump.

Companion: [contract-v1.md](./contract-v1.md), [errors.md](./errors.md), [schemas/](./schemas/), [IMPLEMENTATION_PLAN.md](../IMPLEMENTATION_PLAN.md) §20.

Ground truth for interfaces and deltas: [research-post-v1-seams.md](./research-post-v1-seams.md) (Codex `seek_sequence`, Agents `Chunk`/`apply_diff`, flickzeug unified-only, verify process-group patterns).

## Schema version

Public CLI JSON, execution plans, receipts, and journals use **`version: 2`** when these features are present. v1 JSON remains valid for pure apply/check without plan/transaction fields.

## Flag matrix

| Mode | Mutates root | Acquires lock | Shadow | Notes |
| --- | --- | --- | --- | --- |
| apply (default) | yes | yes | no | Journaled commit |
| `--check` | no | no | no | Exclusive with `--verify` / `--plan` |
| `--plan` | no | no | no | Emits `ExecutionPlan`; exclusive with `--verify` |
| `--verify -- <PROG> [ARG…]` | only on promote | only on promote | yes | Exclusive with `--check` / `--plan` |
| `--verify-shell <SCRIPT>` | only on promote | only on promote | yes | Explicit `/bin/sh -c`; exclusive with argv `--` and with `--check` / `--plan` |
| `--verify-timeout <DURATION>` | n/a | n/a | n/a | Wall clock for verify command (default `600` / `10m`); `Ns`/`Nm`/`Nh` or bare seconds |
| `--verify-output-limit <BYTES>` | n/a | n/a | n/a | Per-stream capture cap (default 8388608); artifacts under shadow |
| `revert <RECEIPT>` | yes | yes | no | New journaled inverse transaction |
| `recover` | maybe | yes | no | Resolves incomplete journal |
| `status` | no | no | no | Health report |
| `doctor` | no | no | no | Env + health |
| `gc [--dry-run]` | objects only | yes | no | Reference-safe |

Mutual exclusions: `--check` ⊕ `--plan` ⊕ (`--verify` | `--verify-shell`) ⊕ apply are distinct modes (check and plan are both read-only but still exclusive with each other for output clarity). `--verify` / `--verify-shell` cannot combine with `--check` or `--plan`, and cannot combine with each other (argv vs shell).

## Verify runner

- Canonical: `--verify -- <PROGRAM> [ARG …]` — no shell.
- Shell escape: `--verify-shell <SCRIPT>` only.
- Defaults: timeout 10 min (`--verify-timeout`), kill grace 5 s, 8 MiB per stream (`--verify-output-limit`).
- Descendant cleanup: verify children run in a new process group; timeout sends SIGTERM then SIGKILL to the group.
- `cwd` = shadow root.
- Env: `AGENT_PATCH_MODE`, `AGENT_PATCH_REAL_ROOT`, `AGENT_PATCH_SHADOW_ROOT`, `AGENT_PATCH_PLAN_DIGEST`, `AGENT_PATCH_INVOCATION_ID`.
- Hard links forbidden in shadows.

Implementation sketch (ground truth: `CommandExt::process_group(0)` + group SIGTERM/SIGKILL; tokio tools often add `kill_on_drop(true)` for timeout orphans):

```rust
use std::os::unix::process::CommandExt;
use std::process::Command;

let mut cmd = Command::new(program);
cmd.args(args)
    .current_dir(&shadow_root)
    .envs(agent_patch_env)
    .process_group(0);
// capture stdout/stderr to bounded artifacts; on deadline: kill(-pgid, SIGTERM) then SIGKILL
```

## Shadow policy

- Default `--shadow-mode=tree`: representative under documented excludes.
- Default excludes: `.agent-patch/`, `.git/`, `target/`, `node_modules/`, `.venv/`, `__pycache__/`, and equivalents.
- `--shadow-include-caches` opts into near-complete trees within budgets.
- `--shadow-mode=touched`: planned paths only; `verify.representative=false`.
- Budgets (defaults): 200_000 files, 20 GiB, 120 s wall.

## Execution plan

Validation freezes one immutable `ExecutionPlan` with a canonical BLAKE3 `plan_digest`. Commit, verify promote, receipt, revert, and recovery consume this plan without rematching.

Canonical encoding: sorted repo-relative paths; no unordered maps in digest-bearing structures; byte-for-byte stable JSON (see [schemas/execution-plan.schema.json](./schemas/execution-plan.schema.json)).

## Matching deltas (opt-in)

1. `--fuzzy=off|rstrip|strip` (default `off`): normalize for search only; still unique-only.
2. `--risk=off|warn|refuse` (default `off`): pure function over `MatchEvidence`.
   - `warn`: apply proceeds; findings appear in success JSON `warnings` (and human stderr-adjacent summary lines).
   - `refuse`: exit `RISK_REFUSED` when findings are non-empty.
3. `--idempotent`: success only if full intended after-state is proven; else `PARTIALLY_APPLIED` (exit 1).
4. Optional `*** Hash: blake3 <hex>` pin per file; pin failure precedes locate (`HASH_PIN_MISMATCH`, exit 5).

Upstream ladder reference (Codex/Agents, **first-match-wins** — not our default):

```text
exact → rstrip (trim_end) → strip (trim) → [Codex only] unicode punctuation normalize
EOF: prefer start at len-pattern.len, then search from cursor
```

agent-patch may enable only the first three levels via `--fuzzy`, and only when the chosen level yields a **unique** hit. Unicode normalize stays deferred. Similarity / `FuzzyConfig.max_fuzz` (flickzeug) never selects a target.

Idempotence is **not** flickzeug `is_diff_applied_with_config` (unified reverse round-trip) and **not** Codex `AppliedPatchDelta::is_exact` (partial-write trust).

## Transaction durability

Before first visible mutation:

1. Acquire `.agent-patch/lock`.
2. Reject unresolved journals (`RECOVERY_REQUIRED`).
3. Persist before-image objects for updates/deletes.
4. Prepare same-directory temps.
5. Write+fsync journal `PREPARED`.

Guarantee: recoverable all-before or all-after — not multi-file atomic visibility.

Journal states: `PREPARED` → `COMMITTING` → `COMPLETED` | `ROLLING_BACK` → `ROLLED_BACK`.

## Receipts

- Every successful mutation writes an internal receipt under `.agent-patch/receipts/`.
- Before-images live in `.agent-patch/objects/<blake3>` — hashes-only receipts are invalid.
- `--receipt <PATH>` exports a copy; `--verify` success does not auto-export a user path.
- Optional `permissions.mode` (Unix bits) and `permissions.executable` are recorded for update/delete; revert prefers `mode` when present.

## Observability

- Optional JSONL event log via `AGENT_PATCH_EVENT_LOG=1` (`.agent-patch/events/events.jsonl`) or a file path.
- Records are metadata-only (phase, ok, transaction_id / plan_digest); never required for correctness.

## Crash / recover

- Incomplete journals block new writers (`RECOVERY_REQUIRED`) until `recover`.
- `COMMITTING` with every path already at after-identity → finalize `COMPLETED`.
- `COMMITTING` / `ROLLING_BACK` with not-all-after (including mixed before/after across files) → restore **all** before-images → `ROLLED_BACK`.
- Paths matching neither before nor after → `RECOVERY_AMBIGUOUS`.
- Dead-PID lock files may be reclaimed; journals are never deleted by lock heuristics.
- Killpoint coverage: `cargo test --features failpoints --test crash_matrix`.

## Oracle caps

- ≤8 candidates; ≤20 lines/excerpt; ≤64 KiB candidate payload; ≤16 KiB repair patch.
- Candidates derived only from locator evidence.
 - On `HUNK_NOT_FOUND` / `HUNK_AMBIGUOUS` with at least one candidate span, JSON may include a capped draft `repair_patch` targeting the first candidate.

## Exit class extensions (still 0–7)

| Exit | Additions |
| --- | --- |
| 0 | `already_applied` success status |
| 1 | `PARTIALLY_APPLIED`, `RISK_REFUSED`, `VERIFY_*` |
| 5 | `HASH_PIN_MISMATCH`, `ROOT_LOCKED`, `CONCURRENT_MODIFICATION` |
| 6 | `RECOVERY_REQUIRED`, `RECOVERY_AMBIGUOUS`, `ROLLBACK_FAILED`, `RECEIPT_OBJECT_MISSING` |
| 7 | `SHADOW_LIMIT_EXCEEDED`, `MATCH_WORK_LIMIT`, existing limits |

## Deferred

`Move File` and `translate` — backlog.
