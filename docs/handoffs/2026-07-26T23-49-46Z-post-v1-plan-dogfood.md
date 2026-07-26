# HANDOFF SUMMARY

## 1) Mission State

- Current objective: Advance `agent-patch` from shipped v1 core into post-v1 reliability (oracle errors, verify-gated commit, receipts, fuzzy-unique, doctor) while keeping fail-closed V4A semantics; leave a clean handoff.
- Current status: **v1 engine complete and dogfooded**; locate→emit, EOF, Codex scenario subset, fuzz/bench/CI dogfood on `main` through `0fae98a`. This session: closed prior open loops; current-state docs rewrite; Cursor skill + direnv PATH; fixture dogfood; archived greenfield plan; **new active** `IMPLEMENTATION_PLAN.md` (post-v1). **Working tree dirty** — docs/plan/skill/fixtures/envrc **not committed** (only this handoff commits per protocol).
- Definition of done: Active plan §21 (oracle, `--plan`, unique-only fuzzy/risk/idempotent, hash pins, receipts/revert, `--verify`, `doctor`, CI green, no silent fuzzy / whole-file fallback). v1 DoD from archived plan already met for core apply.
- Immediate next best action: Commit or stash dirty tree in meaningful groups if user wants clean `main`; then **Phase 0** of `IMPLEMENTATION_PLAN.md` (freeze flag matrix, error/receipt/shadow decisions in contract docs) — or implement **Phase 1 oracle diagnostics** if Phase 0 defaults in §20 are accepted as-is.

## 2) Stable Context (carry forward)

### Product / contract

- Binary/wrapper: `scripts/agent-patch` → `target/release|debug/agent-patch` or `cargo run` — **not on PATH by default**
- With direnv: `.envrc` does `PATH_add scripts` (from `.envrc.example`); bare `agent-patch` resolves to wrapper
- CLI: `--check`, `--json`, `--quiet`, `--root`, `--max-files` (128), `--max-patch-bytes` (4MiB), `--max-file-bytes` (16MiB), `[PATCH_FILE]|stdin`
- Ops: `Add` / `Update` / `Delete`; `@@` / `@@ <anchor>` (exact full line); `*** End of File` (EOF-prefer exact then unique forward); **no** Move until contract + `docs/design/move.md`
- Matching: unique exact; context reduction only if unique; no default whitespace fuzz; locate-all → emit (`engine/locate.rs`, `emit.rs`)
- Newlines: file LF/CRLF wins on update; reject mixed; BOM preserve; Add joins with `\n`
- Hash: BLAKE3 in JSON; exits 0–7 per `docs/errors.md` / `docs/contract-v1.md`
- `CLAUDE.md` → symlink/`AGENTS.md`

### Architecture

```text
CLI → app → parse → path policy → snapshot → validate → plan
     → locate_chunks → emit_chunks → commit → fs
```

### Key paths

- Workspace: `/home/gulp/projects/apply_patch`
- Active plan: `IMPLEMENTATION_PLAN.md`
- Archived plan: `docs/archive/2026-07-greenfield-implementation-plan.md`
- Skill: `.cursor/skills/agent-patch/SKILL.md` + `examples.md`
- Dogfood fixtures: `fixtures/dogfood/` (isolated; use `--root fixtures/dogfood`)
- Codex fixtures: `crates/agent-patch/tests/fixtures/codex-scenarios/`
- Fuzz: `fuzz/`; bench: `crates/agent-patch/benches/apply_update.rs`
- Prior handoff: `docs/handoffs/2026-07-26T23-15-13Z-agent-patch-v1-handoff.md`

### Ground-truth upstream (still valid)

- Codex apply = `seek_sequence` + `similar`; not diffy for V4A
- Agents Python/JS = locate → forward emit; Python newline wins
- flickzeug = unified-diff + `FuzzyConfig`; `is_diff_applied*` not V4A
- Responses `ApplyPatchCall` = headerless per-file `diff` ops (probed)

### User preferences

- Docs/AGENTS: current-state voice (no “we added X”)
- Commit only when asked; handoff protocol commits handoff only
- Prefer `scripts/agent-patch` / dogfood skill for multi-hunk edits
- Keep fixture dogfooding out of crate `src/` when experimenting
- Top-10 post-v1 ideas folded into active `IMPLEMENTATION_PLAN.md`

## 3) Progress So Far (what happened)

- **Attempt:** Take over prior handoff; implement locate→emit.  
  **Result:** `locate.rs` / `emit.rs`; forward cursor; CRLF tests.  
  **Evidence:** `cargo test` green; dogfood 9/9 then 10/10 with EOF.  
  **Decision:** Rematch-after-mutate abandoned.

- **Attempt:** Close open loops (EOF, research, scenarios, fuzz/bench, Move design, commits).  
  **Result:** EOF in contract/protocol; Codex subset ported; `docs/design/move.md`; fuzz + Criterion + CI `scripts/dogfood`; research probes documented.  
  **Evidence:** commits `98c0721` … `0fae98a` on `main`.

- **Attempt:** Current-state docs rewrite (README, AGENTS, protocol, research matrices).  
  **Result:** Docs match shipped behavior; `IMPLEMENTATION_PLAN` status was historical then replaced.  
  **Evidence:** dirty tree still holds latest doc delta (uncommitted).

- **Attempt:** `.cursor/skills/agent-patch` + PATH story.  
  **Result:** Skill for dogfood; `.envrc.example`; user `direnv allow`; bare `agent-patch` works.  
  **Evidence:** `command -v agent-patch` → `…/scripts/agent-patch`; empty run → `PATCH_MISSING_BEGIN`.

- **Attempt:** Fixture dogfood under `fixtures/dogfood`.  
  **Result:** Multi-file apply, EOF, ambiguous fail-closed, `@@` exact-line anchor, `--check` no-write, delete/add.  
  **Evidence:** JSON apply ok; `HUNK_AMBIGUOUS` / stale leave tree unchanged.

- **Attempt:** `@@ fn block_b` without `() {`.  
  **Result:** `HUNK_NOT_FOUND` on anchor.  
  **Decision:** Document anchors as exact full line.

- **Attempt:** Nested `$(<<'PATCH')` for capture.  
  **Result:** bash “unterminated here-document” warning; sometimes still got JSON.  
  **Decision:** Never nest heredocs in `$(...)`.

- **Attempt:** Stale `target/release` during dogfood mid-EOF work.  
  **Result:** `UNKNOWN_OPERATION` for `*** End of File` until rebuild.  
  **Decision:** `scripts/dogfood` always `cargo build --release`; wrapper still does **not** auto-rebuild.

- **Attempt:** Brainstorm 100 ideas → top 10 → new implementation plan.  
  **Result:** Greenfield plan archived; new post-v1 plan §1–21 at repo root.  
  **Evidence:** `IMPLEMENTATION_PLAN.md` ~870 lines; archive ~2565 lines.

## 4) Effective Strategies (helpful)

| Strategy | Why it worked | Where to reuse |
| --- | --- | --- |
| Freeze contract before matching changes | Avoid mid-flight renegotiation | Phase 0 of new plan |
| Locate-all → emit | Matches Agents/Codex; simplifies overlap/CRLF | All engine work |
| `--root fixtures/dogfood` | Safe dogfood without touching crate src | Skill / agent experiments |
| `PATH_add scripts` via direnv | Bare command without global install drift | Checkout UX |
| Codex fixture subset filter | Unique-exact + no overwrite-Add / no Move | Regression corpus |
| Oracle-ready fail-closed errors | Failures already coded; extend with candidates | Phase 1 |
| Recommended defaults in plan §20 | Unblocks coding if user accepts | Phase 0 short-circuit |
| agent-patch for multi-hunk doc edits | Dogfoods tool; atomic multi-file | This repo’s own edits |

## 5) Pitfalls and Anti-Patterns (harmful)

| Pitfall | Why it failed | Avoid next time |
| --- | --- | --- |
| Assume `agent-patch` on PATH | Bare clone has no PATH entry | `scripts/agent-patch` or direnv |
| Stale release binary via wrapper | Prefers release; no rebuild | `cargo build --release` / `doctor` (planned) / dogfood rebuild |
| `@@` anchor substring | Must equal full file line | Copy exact line including `() {` |
| Nested heredoc in `$(...)` | Bash truncates | Stdin heredoc to process only |
| Empty `agent-patch` invocation | `PATCH_MISSING_BEGIN` | Feed patch or `--help` |
| Numeric `@@ -1,3…` as location | Silently ignored as math | Don’t rely on line numbers |
| Blank line inside hunk | Ends hunk early | Space-prefixed context or no blanks |
| Global `cargo install` for in-tree work | Drifts from checkout | Prefer wrapper |
| Whole-file Write after HUNK_* | Forbidden recovery | Regenerate patch from current region |
| Treat archived greenfield plan as active | Superseded | Read root `IMPLEMENTATION_PLAN.md` |

## 6) Open Loops

| Question / issue | Blocking reason | Suggested next probe |
| --- | --- | --- |
| Dirty tree (docs, plan archive move, skill, fixtures, `.envrc`) | Not committed this session | User ask → meaningful commit groups |
| Phase 0 open decisions (§20) | Not frozen in contract docs yet | Accept §20 defaults or decide shadow A/B, verify argv, JSON v1 additive |
| Oracle / `--plan` / `--verify` / receipts | Planned, not implemented | Start Phase 1 after Phase 0 |
| Unique-only `--fuzzy` + risk gate | Needs contract bump | Draft matching delta in `contract-v1.md` |
| Hash pins grammar | Not in protocol yet | Freeze `*** Hash: blake3 …` text |
| Move File | Designed only (`docs/design/move.md`) | Contract bump or keep deferred |
| Wrapper stale-binary without doctor | Doctor is Phase 7 | Interim: rebuild release after engine edits |
| `.envrc` may be gitignored / local-only | User-created; example is in tree | Confirm whether to commit `.envrc` or only example |
| `translate` V4A↔unified | Backlog after `--plan` | Optional Phase 8 |

## 7) Decision Ledger

| Decision | Rationale | Tradeoff |
| --- | --- | --- |
| Custom unique-exact engine (not diffy) | Wrong dialect; Codex doesn’t use diffy for V4A apply | Own matching/fuzz story |
| Locate→emit | Agents/Codex shape; safer overlap | Hunks must match original, not post-mutate buffer |
| EOF supported; Move deferred | High-value protocol gap vs commit complexity | Dialect incomplete vs Codex Move |
| No overwrite Add / no overwrite Move-dest | Stricter than Codex | Lower first-apply vs safety |
| File newline wins | Agents Python ground truth | Reject mixed |
| `scripts/` PATH via direnv, not forced global install | Obvious checkout; avoids drift | Strangers need README/direnv |
| Fixture dogfood under `fixtures/dogfood` | No breaking crate src | Extra tree to maintain |
| Archive greenfield plan; new post-v1 plan | v1 done; next work is reliability/verify | Large historical file retained |
| Post-v1 priority cluster: verify + oracle + receipts + risk | Highest agent leverage on existing seams | Move/translate secondary |
| Shadow strategy recommend A (touched paths) | Lower complexity for `--verify` | Verify cmds must tolerate partial tree |

## 8) Delta Update (for memory/playbook)

### Helpful (+)

- [locate-then-emit] : Resolve hunks on original lines then forward-cursor emit (count: 3)
- [fixture-root-dogfood] : Use `--root fixtures/dogfood` to exercise CLI without touching crate src (count: 2)
- [direnv-scripts-path] : `PATH_add scripts` exposes wrapper as bare `agent-patch` without global install (count: 2)
- [contract-first] : Freeze exits/matching/flags before parallel implementation (count: 2)
- [exact-full-line-anchor] : `@@` anchors must equal the entire file line (count: 2)
- [dogfood-rebuild-release] : Rebuild release before trusting wrapper after engine changes (count: 2)
- [oracle-errors-next] : Extend HUNK_* JSON with candidates/excerpts/repair_patch (count: 1)
- [verify-gated-commit] : Shadow apply + user command before promote (count: 1)

### Harmful (-)

- [stale-release-wrapper] : Wrapper prefers release binary and does not rebuild (count: 2)
- [heredoc-in-substitution] : Nested `$(<<'PATCH')` breaks bash (count: 2)
- [anchor-substring] : Partial `@@` anchor text fails closed (count: 1)
- [path-assumed] : Assuming `agent-patch` on PATH after bare clone (count: 2)
- [whole-file-recovery] : Overwriting files after HUNK failure (forbidden) (count: 2)
- [diffy-as-apply] : Using diffy/flickzeug as V4A apply backend (count: 3)
- [archived-plan-as-active] : Following greenfield IMPLEMENTATION_PLAN after archive (count: 1)

## 9) Next-Agent Brief

- **Read first:** `IMPLEMENTATION_PLAN.md` (active), `AGENTS.md`, `.cursor/skills/agent-patch/SKILL.md`, `docs/contract-v1.md`, `git status` / `git log --oneline -15`.
- **Ignore:** `docs/archive/2026-07-greenfield-implementation-plan.md` as task list (historical); inventing Move/fuzzy-default without contract bump; treating dirty docs as already on `main`.
- **Try first:** (1) Commit dirty groups if user wants clean tree; (2) Phase 0 — write §20 defaults into contract/protocol/errors; (3) Phase 1 — oracle candidates on `HUNK_AMBIGUOUS` / `HUNK_NOT_FOUND` with size caps.
- **Success next turn:** Either clean commits of pending docs/skill/fixtures/plan, or green `cargo test` + `scripts/dogfood` after a Phase 0/1 change that extends JSON errors or freezes the post-v1 contract without weakening unique-exact defaults.

### Working tree note (important)

Uncommitted at handoff time (approx.):

- Modified: `AGENTS.md`, `README.md`, `IMPLEMENTATION_PLAN.md` (replaced), `docs/**` (protocol/contract/design/research/architecture/threat-model)
- Untracked: `.cursor/skills/`, `.envrc`, `.envrc.example`, `docs/archive/`, `fixtures/dogfood/`

Do not assume these are on `main` until committed. Prior session’s engine/EOF/scenarios/fuzz commits **are** on `main` through `0fae98a`.
