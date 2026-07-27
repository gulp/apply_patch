# HANDOFF SUMMARY

## 1) Mission State

- Current objective: Close post-v1 open loops from prior handoff; keep docs/AGENTS accurate in present tense; preserve fail-closed journal/verify/receipt invariants.
- Current status: **Open loops closed and pushed through `d0b0e50`.** Present-tense doc refresh + scrunched `AGENTS.md` hard-won rules are **local uncommitted** on `main`. Repo is **PUBLIC**. Working tree dirty (docs/skills/AGENTS); `?? .envrc` and often `?? .agent-patch/` from local probes — do not commit those.
- Definition of done (session): Prior open loops resolved without regressing `cargo test -p agent-patch` / `scripts/dogfood`; journals never deleted via stale-lock heuristics; docs describe current CLI; handoff committed.
- Immediate next best action: Commit + push the pending docs/`AGENTS.md`/skill updates (or discard if undesired). Optional backlog only: `Move File`, `translate`, soak CI beyond crash_matrix.

## 2) Stable Context (carry forward)

- Workspace: `/home/gulp/projects/apply_patch` (Rust; crate `crates/agent-patch`)
- Branch: `main` @ `d0b0e50` tracks `origin/main` (uncommitted docs ahead locally)
- Remote: `https://github.com/gulp/apply_patch.git` (**PUBLIC**)
- Canonical CLI: `scripts/agent-patch`; contracts `docs/contract-v1.md` + `docs/contract-v2.md`
- Store: `<root>/.agent-patch/{lock,objects,transactions,receipts,shadows,events}`
- Agent playbook: `AGENTS.md` (symlink `CLAUDE.md`) includes **Hard-won rules**
- Gates: `scripts/dogfood` (T1–T13); `cargo test --features failpoints --test crash_matrix`; CI runs both
- Env: `AGENT_PATCH_EVENT_LOG=1` or path; failpoints `AGENT_PATCH_FAILPOINT=<name>` with `--features failpoints`
- Killpoints: `after_prepared`, `before_visible_mutate`, `after_first_visible`, `before_completed`
- Deferred by contract: `Move File`, `translate` only (verify-shell shipped)
- User prefs: present-tense docs; detailed commits when asked; never commit `.envrc`; scrunchy AGENTS curation

## 3) Progress So Far (what happened)

- Attempt: Resume from `2026-07-27T00-53-03Z-post-v1-ship-handoff.md` brief  
  Result: Pushed handoff `2975619`; confirmed repo was private then later made public  
  Evidence: `git push`; `gh repo view` → PUBLIC  
  Decision: Advance hardening, not invent unrelated features

- Attempt: Extend `scripts/dogfood` with T8–T13 (plan/verify/receipt/recover/idempotent)  
  Result: 16/16 after fixing CLI ordering  
  Evidence: Initial fail — verify put patch after `--` → empty stdin; revert used top-level `--json --root` → wrong root / `RECEIPT_OBJECT_MISSING`  
  Decision: `run_verify` with patch before `--`; `revert --json --root …`

- Attempt: Close remaining open loops (failpoints, recover, verify-shell, events, mode bits, visibility)  
  Result: Commits `4a9d968` (dogfood), `67db709` (failpoints/recover/lock), `d0b0e50` (verify-shell/events/modes) pushed  
  Evidence: crash_matrix 5/5 after stale-PID lock reclaim; dogfood 16/16; clippy `-D warnings` cleaned  
  Decision: Fix recover mixed before/after → NeedsRollback (all-before); reclaim dead-PID locks only

- Attempt: Present-tense docs update for shipped surface  
  Result: README/AGENTS/contract-v2/design/threat-model/skill updated locally  
  Evidence: `git diff --stat` ~13 files; not committed yet  
  Decision: Frame as current state; remove `--verify-shell` from deferred

- Attempt: Scrunch conversation learnings into `AGENTS.md`  
  Result: **Hard-won rules** Facts/Do/Don't added; overlap with edit section pruned  
  Evidence: `AGENTS.md` § Hard-won rules  
  Decision: Keep short; exact paths/commands

## 4) Effective Strategies (helpful)

- Strategy: Close loops in vertical slices (dogfood → durability → ops flags → docs → AGENTS)  
  - Why it worked: Each slice had a clear gate  
  - Where to reuse: Post-ship hardening

- Strategy: Failpoint abort in child + parent `recover` matrix  
  - Why it worked: Exercises real incomplete journals without inventing SIGKILL harness  
  - Where to reuse: New journal states / commit phases

- Strategy: Stale lock = dead PID reclaim; never touch journals  
  - Why it worked: Unblocked crash_matrix after `abort()` skipped `RootLock` Drop  
  - Where to reuse: Any advisory lock + crash tests

- Strategy: Recover mixed before/after as all-before restore  
  - Why it worked: Matches design table; multi-file mid-commit no longer stuck Ambiguous  
  - Where to reuse: Any further recovery table edits

- Strategy: Dogfood helpers encode clap quirks (`patch` before `--`; subcommand-local flags)  
  - Why it worked: Caught real agent/CLI footguns  
  - Where to reuse: New subcommands / `last` argv flags

- Strategy: Present-tense docs + scrunchy AGENTS hard-won rules  
  - Why it worked: Next agent gets product truth + session pitfalls without reading full transcripts  
  - Where to reuse: End of each major phase

## 5) Pitfalls and Anti-Patterns (harmful)

- Pitfall: `agent-patch --json --root R --verify -- false patch`  
  - Why it failed: Patch absorbed into verify argv; stdin empty → `PATCH_MISSING_BEGIN`  
  - How to avoid: `--verify PATCH -- false` or dedicated helper

- Pitfall: `agent-patch --json --root R revert receipt`  
  - Why it failed: Top-level flags ignored by subcommand; wrong cwd/root → missing objects  
  - How to avoid: `revert --json --root R receipt`

- Pitfall: Treating mixed mid-commit as `RECOVERY_AMBIGUOUS` when every path is before or after  
  - Why it failed: Violates journal design table; blocks recover  
  - How to avoid: Restore all before-images; Ambiguous only when content matches neither

- Pitfall: `abort()` leaving lock file with dead PID  
  - Why it failed: `recover` timed out on `ROOT_LOCKED`  
  - How to avoid: `lock_holder_dead` + reclaim; never delete journals that way

- Pitfall: Clap `trailing_var_arg` + `last` together  
  - Why it failed: Exit 101 panic (prior session; still in AGENTS)  
  - How to avoid: `last = true` alone

- Pitfall: Leaving `ROLLING_BACK` after successful in-process rollback  
  - Why it failed: Blocks all future applies  
  - How to avoid: Terminal `ROLLED_BACK`

- Pitfall: Whole-file rewrite / committing `.envrc` / claiming multi-file atomic visibility  
  - Why it failed: Project invariants / ephemeral local state / contract wording  
  - How to avoid: See AGENTS Hard-won **Don't**

## 6) Open Loops

- Question / issue: Uncommitted present-tense docs + `AGENTS.md` hard-won rules + skill updates  
  - Blocking reason: User asked for docs/AGENTS edits but not explicitly to commit this batch  
  - Suggested next probe: `git add` those paths (not `.envrc` / `.agent-patch`) → commit → push

- Question / issue: `Move File` / `translate` still backlog  
  - Blocking reason: Contract deferred; out of scope for open-loop close  
  - Suggested next probe: Only with explicit contract bump

- Question / issue: Broader crash soak / process-leak CI beyond killpoint matrix  
  - Blocking reason: Phase 8 soak not fully instrumented; crash_matrix covers named failpoints  
  - Suggested next probe: Optional CI soak job if user wants more than matrix

- Question / issue: Accidental root `.agent-patch/` from local CLI probes  
  - Blocking reason: Ephemeral  
  - Suggested next probe: `rm -rf .agent-patch` before commit; ensure gitignore if needed

## 7) Decision Ledger

- Decision: Ship `--verify-shell` while closing open loops (was deferred)  
  - Rationale: Explicit open loop + contract already specified shape  
  - Tradeoff: Shell escape exists; argv remains preferred

- Decision: Mixed COMMITTING states restore all-before when every path is before or after  
  - Rationale: Align recover with `transaction-journal.md` table  
  - Tradeoff: Never invent mixed after roll-forward

- Decision: Dead-PID lock reclaim only  
  - Rationale: Crash tests + real abort paths; journals untouched  
  - Tradeoff: Live-PID lock still blocks (correct)

- Decision: Receipt `permissions.mode` optional alongside `executable`  
  - Rationale: Exact revert fidelity without invalidating older receipts  
  - Tradeoff: Schema allows missing `mode`

- Decision: Make GitHub repo public  
  - Rationale: Close visibility open loop (created private by default)  
  - Tradeoff: Code/docs publicly visible

- Decision: Scrunch session learnings into `AGENTS.md` Hard-won rules  
  - Rationale: User asked for shortest possible curated playbook  
  - Tradeoff: Detail lives in handoffs/contracts; AGENTS stays operational

## 8) Delta Update (for memory/playbook)

### Helpful (+)

- [verify-patch-before-dashdash] : Place patch path before `--` for `--verify` (count: 2)
- [subcommand-local-flags] : Put `--json`/`--root` on subcommands (`revert`, `status`, …) (count: 2)
- [recover-mixed-all-before] : Mixed before/after across files → restore all before-images (count: 2)
- [stale-lock-dead-pid] : Reclaim lock only when holder PID is dead; never delete journals that way (count: 2)
- [failpoint-crash-matrix] : `AGENT_PATCH_FAILPOINT` + `--features failpoints` crash_matrix (count: 1)
- [journal-before-mutate] : Durable PREPARED + CAS before visible rename/delete (count: 4)
- [clap-last-argv] : `last = true` alone for verify argv (count: 3)
- [docs-present-tense] : Document current behavior without “we added” voice (count: 3)
- [agents-hard-won] : Scrunch pitfalls into AGENTS Hard-won rules (count: 1)
- [scripts-agent-patch] : Prefer `scripts/agent-patch` for multi-hunk; rebuild release after engine changes (count: 5)

### Harmful (-)

- [verify-patch-as-argv] : Do not put patch file after `--` under `--verify` (count: 2)
- [toplevel-flags-with-subcommand] : Do not rely on top-level `--json/--root` before `revert`/`recover` (count: 2)
- [ambiguous-on-mixed-commit] : Do not classify proven before|after mixes as RECOVERY_AMBIGUOUS (count: 1)
- [rolling-back-forever] : Do not leave ROLLING_BACK after successful in-process rollback (count: 3)
- [whole-file-rewrite] : Never recover from HUNK_* by overwriting entire files (count: 4)
- [commit-envrc] : Do not commit local `.envrc` (count: 2)
- [multi-file-atomicity-claim] : Do not claim multi-file atomic visibility (count: 3)

## 9) Next-Agent Brief

- What to read first: This handoff; `AGENTS.md` Hard-won rules; `docs/contract-v2.md`; `git status -sb`; `git log -5 --oneline`
- What to ignore: Untracked `.envrc` / stray `.agent-patch/`; inventing Move/translate without contract bump; re-opening closed loops already on `origin/main`
- What to try first: Commit and push pending docs/AGENTS/skill changes if still desired; confirm `scripts/dogfood` + crash_matrix green after any further code touch
- What success looks like in the next turn: Clean tree (except `.envrc`) aligned with `origin/main`; no journal/lock hand-deletion; gates green; present-tense docs match CLI `--help`
