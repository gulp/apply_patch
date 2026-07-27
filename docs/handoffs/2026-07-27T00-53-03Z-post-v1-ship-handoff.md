# HANDOFF SUMMARY

## 1) Mission State

- Current objective: Implement and ship post-v1 `agent-patch` reliability (per `IMPLEMENTATION_PLAN.md`): contract freeze → oracle/plan → journaled commit → receipts/revert/GC → shadow/verify → fuzzy/risk/pins/idempotent → doctor/docs; keep dogfood green; commit and push.
- Current status: **Done for Phases 0–8 implementation + present-tense docs + three logical commits pushed to `origin/main`.** Working tree clean except untracked local `.envrc`.
- Definition of done (from plan / user): Fail-closed V4A apply preserved; verify-gated promote; self-contained receipts/revert; crash-recoverable journals; unique-only opt-in fuzzy; docs describe current behavior; tests/dogfood green; changes committed and pushed.
- Immediate next best action: Either (a) harden remaining DoD gaps (killpoint/crash-soak CI, `--verify-shell`, event logs, fuller killpoint recovery matrix), or (b) user-directed polish (make GitHub repo public, expand dogfood fixtures for verify/oracle/revert). Do **not** commit `.envrc`.

## 2) Stable Context (carry forward)

- Workspace: `/home/gulp/projects/apply_patch` (Rust workspace; crate `crates/agent-patch`)
- Branch: `main` @ `db3297b53180f6cb7de4bbb604ef4dd188e1f1e8`, tracks `origin/main`
- Remote: `https://github.com/gulp/apply_patch.git` (**private** repo created during push because none existed)
- Canonical CLI: `scripts/agent-patch` (release → debug → `cargo run`); rebuild release after engine changes
- Contracts: `docs/contract-v1.md` (baseline unique-exact), `docs/contract-v2.md` (plans/verify/journals/receipts/fuzzy/risk/idempotent)
- Schemas: `docs/schemas/{execution-plan,receipt,journal}.schema.json` + `crates/agent-patch/tests/schema_freeze.rs`
- Journal design: `docs/design/transaction-journal.md`; store under `<root>/.agent-patch/{lock,objects,transactions,receipts,shadows}`
- Seam ground truth: `docs/research-post-v1-seams.md`
- Active plan: `IMPLEMENTATION_PLAN.md`
- Edit rule: prefer `scripts/agent-patch` for multi-hunk; never whole-file rewrite after `HUNK_*`; regenerate from current content
- `CLAUDE.md` is a symlink to `AGENTS.md`
- User prefs: present-tense docs (no “we added”); detailed commit messages; do not commit ephemeral files (`.envrc`); fail-closed; unique match never first-match-wins; hard links forbidden in shadows
- Verify: `--verify -- <PROG> [ARG…]` only (argv); `--verify-shell` deferred in contract-v2
- Revert is subcommand `revert <RECEIPT>`, not `--revert`
- Dependency: `libc = "0.2"` for Unix process-group verify kill

## 3) Progress So Far (what happened)

- Attempt: Continue post-v1 implementation after Phase 0–2 (contract freeze docs; oracle/`--plan`; store modules uncommitted)
- Result: Phases 3–8 implemented in code
- Evidence: `cargo test -p agent-patch` green; `scripts/dogfood` 10/10; release rebuilt
- Decision: Wire journal into `commit_plan` before visible mutation; leave COMPLETED journals; incomplete blocks via `refuse_if_incomplete`

- Attempt: Phase 3 journaled commit + `recover` + `status`
- Result: Lock → put_object → PREPARED → COMMITTING → inner rename commit → COMPLETED + receipt path; CLI subcommands `status`/`recover`
- Evidence: unit tests in `journal`/`recover`/`status`; CLI `apply_writes_completed_journal_and_status_ok`, `recover_clears_prepared_incomplete`
- Decision: On in-process failure mark `ROLLED_BACK` (unless `ROLLBACK_FAILED` → leave incomplete); codex fixtures ignore `.agent-patch/` in tree compare

- Attempt: Phase 4 receipts / revert / GC
- Result: Internal receipts under `.agent-patch/receipts/`; `--receipt` export; `revert`; `gc [--dry-run]`
- Evidence: CLI `apply_receipt_export_and_revert`; `gc` orphan delete dry-run/live tests
- Decision: Hashes-only receipts rejected; revert proves after-states then journaled inverse

- Attempt: Phase 5–6 shadow + verify
- Result: `shadow.rs` tree/touched; `verify.rs` process-group + bounded streams; promote via `commit_plan` only on verify ok
- Evidence: CLI `verify_promotes_on_success_and_skips_on_failure`; clap fixed by using `last = true` **without** `trailing_var_arg` (debug assert panic otherwise)
- Decision: Default excludes `.agent-patch`, `.git`, `target`, `node_modules`, etc.; hard links forbidden

- Attempt: Phase 7 fuzzy / risk / pins / idempotent
- Result: `match_opts.rs`; fuzzy ladder in matcher; hash pin parse; `--idempotent` assess before validate; risk refuse on weak evidence
- Evidence: CLI `fuzzy_rstrip_unique_only`, `hash_pin_mismatch_fails_before_apply`, `idempotent_replay_succeeds`
- Decision: Fuzzy only after exact/context-reduction miss; ambiguous at fuzzy still fails closed

- Attempt: Phase 8 doctor + docs
- Result: `doctor` subcommand; README/AGENTS/skill/protocol/architecture/design/threat-model updated present-tense
- Evidence: docs commit `db3297b`
- Decision: Product docs last after code so voice matches shipped CLI

- Attempt: Commit all changes in logical groups and push
- Result: Three commits; created private `gulp/apply_patch` and pushed (`gh repo create … --push`)
- Evidence: `origin/main` == `db3297b`
- Decision: Skip `.envrc`; do not invent Co-Authored trailers without known noreply

## 4) Effective Strategies (helpful)

- Strategy: Implement store modules first, then wrap `commit_plan` (PREPARED before rename) rather than rewriting the whole commit body  
  - Why it worked: Kept existing prepare/rename/rollback logic; tests stayed green  
  - Where to reuse: Further durability features (postcondition verify, failpoints)

- Strategy: Prefer `locate_chunks_with` / `apply_update_with` + `MatchOptions::default()` wrappers  
  - Why it worked: Avoided breaking every unit test call site while threading fuzzy/risk  
  - Where to reuse: Any new match policy knobs

- Strategy: Clap verify argv as `#[arg(last = true, allow_hyphen_values = true)]` only  
  - Why it worked: Clap 4 forbids combining `trailing_var_arg` + `last` (exit 101 panic)  
  - Where to reuse: Any `-- <argv>` style flags

- Strategy: Ignore `.agent-patch/` in codex scenario tree compares  
  - Why it worked: Journals/objects would otherwise fail fixture equality  
  - Where to reuse: Any fixture that compares full trees after apply

- Strategy: Docs in three layers — contract freeze → code → present-tense product docs  
  - Why it worked: Clear commit story; product docs not written against unfinished CLI  
  - Where to reuse: Future contract bumps

- Strategy: Create private GitHub remote with `gh repo create --source=. --remote=origin --push` when push requested and no origin  
  - Why it worked: Unblocked push without inventing URLs  
  - Where to reuse: Other local-only repos under `gulp`

## 5) Pitfalls and Anti-Patterns (harmful)

- Pitfall: Marking failed commits `ROLLING_BACK` forever after successful in-process rollback  
  - Why it failed: `refuse_if_incomplete` would block all future applies  
  - How to avoid: Terminal `ROLLED_BACK` when in-process rollback succeeded; leave incomplete only for `ROLLBACK_FAILED` / crash mid-COMMITTING

- Pitfall: `Arg::trailing_var_arg` + `Arg::last` together  
  - Why it failed: Clap debug assert aborts with exit 101 on every invocation  
  - How to avoid: Use `last = true` alone for verify argv

- Pitfall: Whole-file rewrite when patch/commit fails  
  - Why it failed: Violates project invariants and AGENTS rules  
  - How to avoid: Regenerate localized patches from current content; use `scripts/agent-patch`

- Pitfall: Assuming `git remote` / `origin` exists  
  - Why it failed: Earlier session push failed silently conceptually; this session had zero remotes  
  - How to avoid: Check `git remote -v` before push; create with `gh` if user asked to push

- Pitfall: Committing `.envrc`  
  - Why it failed: Local machine PATH/direnv; ephemeral  
  - How to avoid: Leave untracked (as now)

- Pitfall: Claiming multi-file atomic *visibility*  
  - Why it failed: Contract guarantees recoverability to all-before/all-after, not instantaneous multi-file visibility  
  - How to avoid: Keep wording aligned with `docs/design/transaction.md` / contract-v2

## 6) Open Loops

- Question / issue: Killpoint / failpoint crash matrix and soak CI not implemented  
  - Blocking reason: Phase 3 acceptance asks for killpoint sweep; only unit/CLI recover paths covered  
  - Suggested next probe: Add failpoint feature + SIGKILL matrix across journal/rename; CI job

- Question / issue: `--verify-shell` not in CLI  
  - Blocking reason: Deferred in contract-v2; argv-only shipped  
  - Suggested next probe: Only if user asks; keep security warning

- Question / issue: Event log / retention cleanup / `--verify` artifact retention policies thin  
  - Blocking reason: Phase 8 partial (doctor exists; optional JSONL events not wired)  
  - Suggested next probe: `AGENT_PATCH_EVENT_LOG` adapter if orchestrators need it

- Question / issue: Repo is **private** on GitHub  
  - Blocking reason: Created private by default when no remote existed  
  - Suggested next probe: User may want `gh repo edit --visibility public`

- Question / issue: Dogfood fixtures do not yet cover verify/oracle/revert/recover end-to-end  
  - Blocking reason: Plan §18 lists extending `fixtures/dogfood`; current dogfood still classic apply gates  
  - Suggested next probe: Add dogfood cases for `--verify -- true`, recover, revert

- Question / issue: Permissions on revert restore default to 0o644/0o755 from receipt `executable` flag only  
  - Blocking reason: Full mode bits not stored beyond executable boolean in receipt schema  
  - Suggested next probe: Expand receipt permissions if exact mode fidelity is required

## 7) Decision Ledger

- Decision: Unique-exact default; `--fuzzy` unique-only (never first-match)  
  - Rationale: Fail-closed vs Codex/Agents ladders  
  - Tradeoff: Agents must regenerate patches more often than silent fuzzy tools

- Decision: Durable PREPARED journal + CAS before-images before visible mutation  
  - Rationale: Reject Codex partial-leave-changes shape  
  - Tradeoff: Apply creates `.agent-patch/` store; more I/O/fsync

- Decision: Default shadow `tree` with cache excludes; `touched` labeled non-representative  
  - Rationale: Usable verify on large checkouts without hard links  
  - Tradeoff: Excluded `target/`/`node_modules` may break some verify cmds unless `--shadow-include-caches`

- Decision: Verify promote locks only at promote; check/plan lock-free  
  - Rationale: Contract-v2 flag matrix  
  - Tradeoff: Shadow work can race with concurrent writers until promote revalidates

- Decision: Three commits (contract → code → docs) then create private `gulp/apply_patch` and push  
  - Rationale: Logical history; user required push; no origin existed  
  - Tradeoff: Private repo may need visibility change; monolithic code commit (app/cli/main intertwined)

- Decision: Leave `.envrc` untracked  
  - Rationale: Local-only  
  - Tradeoff: Contributors still use `.envrc.example`

## 8) Delta Update (for memory/playbook)

### Helpful (+)

- [journal-before-mutate] : Write durable PREPARED journal and CAS before-images before any visible rename/delete (count: 3)
- [clap-last-argv] : Use clap `last = true` alone for `-- <argv>`; never combine with `trailing_var_arg` (count: 2)
- [ignore-agent-patch-in-fixtures] : Exclude `.agent-patch/` from scenario tree equality checks (count: 2)
- [match-opts-wrappers] : Keep `*_with(opts)` + default wrappers to avoid mass test churn (count: 2)
- [docs-after-code] : Ship present-tense product docs only after CLI surface is stable (count: 2)
- [gh-create-on-push] : If user asks to push and no remote exists, `gh repo create … --remote=origin --push` (count: 1)
- [scripts-agent-patch] : Prefer `scripts/agent-patch` for multi-hunk edits; rebuild release after engine changes (count: 4)

### Harmful (-)

- [rolling-back-forever] : Do not leave ROLLING_BACK after successful in-process rollback — blocks all applies (count: 2)
- [whole-file-rewrite] : Never recover from HUNK_* by overwriting entire files (count: 3)
- [assume-origin] : Do not assume git remotes exist; verify before push (count: 2)
- [commit-envrc] : Do not commit local `.envrc` (count: 1)
- [multi-file-atomicity-claim] : Do not claim multi-file atomic visibility; claim durable recoverability (count: 2)

## 9) Next-Agent Brief

- What to read first: This handoff; `IMPLEMENTATION_PLAN.md` §17–20; `docs/contract-v2.md`; `git log -5 --oneline`; `scripts/agent-patch --help`
- What to ignore: Untracked `.envrc`; archived greenfield plan except for historical invariants; inventing `--verify-shell` unless requested
- What to try first: If continuing hardening — killpoint/recover matrix or dogfood verify/revert cases; if ops — confirm whether `gulp/apply_patch` should stay private
- What success looks like in the next turn: Clear user-directed goal advanced without regressing `cargo test -p agent-patch` / `scripts/dogfood`; no whole-file rewrite recovery; journals never deleted by stale-lock heuristics
