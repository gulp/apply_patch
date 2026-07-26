# HANDOFF SUMMARY

## 1) Mission State

- Current objective: Deliver `agent-patch` — a repo-local Rust CLI for fail-closed, transactional V4A-style patches for coding agents; document design/contract from primary sources; leave a clean handoff for the next pass.
- Current status: **v1 CLI implemented and dogfooded**; git history on `main` (8 commits through design/research); design docs + `AGENTS.md` rewritten against ground truth; **working tree dirty** with those doc/AGENTS updates + new `docs/research-next-pass.md` **not yet committed** (only this handoff is committed per protocol).
- Definition of done (from plan §21 / contract): documented protocol; deterministic Add/Update/Delete; fail-closed on bad/stale/ambiguous/unsafe; `--check` parity; transactional commit + rollback; stable exits/JSON; CI; agents use `scripts/agent-patch` without MCP. Fuzz/benches/Phase-8 multi-harness dogfood and locate→emit refactor still open.
- Immediate next best action: Commit remaining doc delta (`AGENTS.md`, `docs/design/*`, `docs/research-next-pass.md`, `docs/architecture.md`) if desired; then implement **locate-all → cursor emit** + **CRLF matrix tests**, or parse **`*** End of File`** (highest-value protocol gap).

## 2) Stable Context (carry forward)

### Product / contract

- Binary/wrapper: `scripts/agent-patch` → `target/release|debug/agent-patch` or `cargo run`
- CLI: `--check`, `--json`, `--quiet`, `--root`, `--max-files` (128), `--max-patch-bytes` (4MiB), `--max-file-bytes` (16MiB), `[PATCH_FILE]|stdin`
- Ops v1: `*** Add File` / `*** Update File` / `*** Delete File` only; **no** `*** Move to:` / `*** End of File` until contract bump
- Matching: unique exact; context reduction only if unique; no default whitespace/unicode fuzz; no first-match-wins
- Newlines: preserve LF/CRLF on update (file wins); reject mixed; BOM preserve; Add joins with `\n`
- Hash: BLAKE3 labeled in JSON; exits 0–7 per `docs/errors.md` / `docs/contract-v1.md`
- `CLAUDE.md` → symlink to `AGENTS.md`

### Architecture

```text
CLI → app → parse → path policy → snapshot → validate → plan/engine → commit → fs
```

- Apply engine: custom (not `diffy`/`flickzeug`); observational `similar`
- Target apply shape: locate chunks on original lines → forward cursor emit (`docs/design/apply-engine.md`)
- Commit: in-memory plan → revalidate blake3 → temps + rename → rollback (`docs/design/transaction.md`)

### Ground-truth upstream (verified)

| Fact | Where |
| --- | --- |
| Codex apply = `seek_sequence` + `similar`; **not** diffy | `~/.opensrc/.../openai/codex/main/codex-rs/apply-patch/` |
| Codex/`zed` `diffy` = unified-diff **display** | `codex-rs/tui/src/diff_render.rs`; `codex-acp/src/thread.rs` |
| Agents `apply_diff`/`applyDiff` = locate + `_apply_chunks` / `applyChunks` | `openai-agents-python/.../apply_diff.py`; `openai-agents-js/.../applyDiff.ts` |
| Python Agents: file newline wins; JS always `\n` | respective tests/impl |
| Codex non-transactional (`015_*leaves_changes`); Add may overwrite | `codex-rs/apply-patch/tests/fixtures/scenarios/` |
| `flickzeug` = diffy fork; unified + `FuzzyConfig`; not V4A | `opensrc path crates:flickzeug` → `prefix-dev/flickzeug@0.5.2` |
| V4A also in Aider `patch_coder.py`, gpt-oss, OpenClaw path extract, OpenCode | grep-app + opensrc |

### Repo layout

- Workspace: `/home/gulp/projects/apply_patch` — `crates/agent-patch/`, `scripts/`, `docs/`, `.github/workflows/ci.yml`
- Key docs: `README.md`, `AGENTS.md`, `docs/contract-v1.md`, `docs/design/`, `docs/research-*.md`, `IMPLEMENTATION_PLAN.md` (historical)

### User preferences

- Docs/AGENTS: current-state voice (no “we added X”)
- Operational instructions only in `AGENTS.md`; product in `README.md`
- Commit only when asked; meaningful commit groups when committing
- Prefer primary sources (`opensrc`, grep-app) over plan assumptions

## 3) Progress So Far (what happened)

- **Attempt:** Implement `IMPLEMENTATION_PLAN.md` greenfield Rust CLI.  
  **Result:** Working crate + CLI; unit + integration tests; release binary.  
  **Evidence:** `cargo test --workspace` green; smoke apply changed `TIMEOUT_SECS`; clippy `-D warnings` with `#![allow(clippy::result_large_err)]`.  
  **Decision:** Custom matcher (not diffy); Move deferred; BLAKE3; transactional commit.

- **Attempt:** Phase-8 dogfood + `scripts/dogfood`.  
  **Result:** 9/9 scenarios pass (single/multi-hunk/multi-file, stale, ambiguous, add/delete, path escape, check→apply).  
  **Evidence:** `scripts/dogfood` → `dogfood: 9 passed, 0 failed`.

- **Attempt:** Study Codex + Zed ACP for “diffy apply path”.  
  **Result:** Corrected false assumption — diffy is display; apply is custom/`similar`.  
  **Evidence:** `apply-patch/Cargo.toml` has `similar` not `diffy`; ACP `parse_patch` + `diffy::Patch::from_str` for UI.  
  **Decision:** Keep custom engine; document in research + design.

- **Attempt:** Study Agents Python/JS `apply_diff` / `applyDiff`.  
  **Result:** Adopted locate→emit as target engine shape; Python newline policy preferred.  
  **Evidence:** `apply_diff.py` / `applyDiff.ts` + tests.

- **Attempt:** `opensrc openai/agents` and `opensrc @openai/agents`.  
  **Result:** Failed (wrong repo / npm decode).  
  **Evidence:** Error messages; resolved via `npm view` → `openai/openai-agents-js`.

- **Attempt:** Design docs under `docs/design/`; amalgamate AGENTS/CLAUDE; expand README.  
  **Result:** Design suite + symlink `CLAUDE.md`→`AGENTS.md`.  
  **Decision:** Product in README; ops in AGENTS.

- **Attempt:** `git init` + 8 meaningful commits.  
  **Result:** Clean `main` through design/research commit `78e6b88`; later doc/AGENTS ground-truth rewrite left **uncommitted**.

- **Attempt:** Dogfood opensrc + grep-app on unknowns (flickzeug, EOF, Move, Aider, OpenClaw).  
  **Result:** `docs/research-next-pass.md`; AGENTS “Code reference search tools” section; design rewritten to fact tables.  
  **Evidence:** Cached trees under `~/.opensrc/`; grep-app hits for markers/APIs.

## 4) Effective Strategies (helpful)

| Strategy | Why it worked | Where to reuse |
| --- | --- | --- |
| Freeze `docs/contract-v1.md` before deep code | Shared exits/limits/matching without mid-flight renegotiation | Greenfield CLIs with public dialects |
| Layer parse → pure apply → commit → FS | String-testable match; injectable writes | Patch/migrate tools |
| opensrc + read **call sites**, not only manifests | Exposed diffy≠apply | Dependency inspiration |
| npm view → GitHub when opensrc npm fails | Unblocked `@openai/agents` | Broken registry fetches |
| grep-app literal markers → opensrc full tree | Fast discovery of V4A adopters | Cross-ecosystem protocol research |
| `scripts/dogfood` scenario gate | One-command acceptance for stale/ambiguous/safety | Phase-8 style gates |
| Design after research, before big refactor | Captured locate→emit + transactional delta without thrash | Post-research refactors |
| `CLAUDE.md`→`AGENTS.md`; README=product | Stops dual-doc drift | Multi-harness repos |
| Current-state doc voice | User requirement; less noise | All user-facing markdown |

## 5) Pitfalls and Anti-Patterns (harmful)

| Pitfall | Why it failed | Avoid next time |
| --- | --- | --- |
| Plan text “diffy/flickzeug backend” as settled | Upstream apply isn’t that | Verify call graph before locking stack |
| `opensrc openai/agents` | Repo does not exist | `openai-agents-python` / `-js` |
| Assume npm package = apply implementation | Umbrella re-exports `agents-core` | Open exports + monorepo paths |
| Heredoc inside `$(...)` | Bash truncates / warns | Patch files or stdin heredoc to binary |
| Mutate buffer then rematch per hunk | Harder overlap/CRLF than locate→emit | Precompute chunks, then emit |
| Fragile `commit.rs` (`mem::forget`, duplicate cleanup) | Temp lifecycle bugs | Own `TempHandle` through rename |
| Partial StrReplace on contract | Dropped Semantics table | Re-read full file after structural edits |
| Infer algorithm from Cargo dep name | Codex workspace lists `diffy` for TUI | Always rg call sites |
| Feed V4A to `diffy`/`flickzeug` apply | Unified-diff APIs only | Custom V4A engine only |

## 6) Open Loops

| Question / issue | Blocking reason | Suggested next probe |
| --- | --- | --- |
| Locate-all → emit not fully refactored in code | Design target; impl still rematch-oriented in places | Implement `locate_chunks`/`emit_chunks`; keep `apply_update` seam |
| `*** End of File` unsupported | Deferred in contract | Port Codex/Agents EOF-prefer exact locate |
| `*** Move to:` unsupported | Deferred; collision/rollback design | Codex scenarios `004_*` / `010_*`; OpenClaw path extract |
| Uncommitted design/AGENTS/next-pass docs | Not committed after rewrite | `git status` + commit when user asks |
| Fuzz targets / Criterion benches / macOS CI dogfood | Scaffold incomplete | Plan §14–17 / Phase 7–8 |
| Responses API `ApplyPatchCall` schema | Not probed | grep-app in `openai-node` / `openai-python` |
| Already-applied detection | Unprobed | flickzeug `is_diff_applied*`; Codex `delta.exact` |
| Port Codex scenario corpus (exact subset) | Not started | Filter scenarios incompatible with unique-exact / no overwrite-Add |

## 7) Decision Ledger

| Decision | Rationale | Tradeoff |
| --- | --- | --- |
| Custom exact matcher, not diffy/flickzeug | Wrong dialect; Codex itself doesn’t use them for V4A apply | No free unified-diff fuzz; must own matching |
| Unique match fail-closed | Safety / I5 vs agent first-apply rate | Lower success vs Codex/Agents fuzz |
| Transactional commit + rollback | Differentiator vs Codex `015` partial leave | More commit complexity |
| Add never overwrites | Fail closed | Stricter than Codex |
| Move + EOF deferred | v1 scope; both need careful commit semantics | Dialect incompleteness vs upstream prompts |
| BLAKE3 fingerprints | Contract recommendation | Labeled algorithm in JSON |
| `similar` observational only | Same as Codex apply-patch | Not used for correctness |
| File newline wins on update | Agents Python ground truth | Reject mixed; Add still LF |
| AGENTS ops-only + CLAUDE symlink | User request; OpenAI Agents pattern | Dual entrypoint via symlink only |

## 8) Delta Update (for memory/playbook)

### Helpful (+)

- [verify-call-sites] : Do not trust Cargo/npm deps for apply algorithm; read call sites (count: 3)
- [opensrc-github-fallback] : If opensrc npm fails, `npm view repository` then opensrc GitHub (count: 2)
- [locate-then-emit] : Resolve hunk positions on original lines, then cursor-emit (Agents/Codex) (count: 2)
- [dogfood-script] : Encode Phase-8 scenarios in `scripts/dogfood` (count: 1)
- [contract-first] : Freeze exits/matching/limits before parallel modules (count: 1)
- [docs-current-state] : Write docs as present tense current state (count: 2)
- [grep-app-literals] : Search literal markers/APIs, then opensrc full trees (count: 2)

### Harmful (-)

- [diffy-as-apply] : Assuming diffy/flickzeug apply V4A (count: 3)
- [wrong-opensrc-name] : `openai/agents` is invalid; use python/js repos (count: 2)
- [heredoc-in-substitution] : Nested `$(<<'PATCH')` breaks bash (count: 1)
- [partial-tree-on-failure] : Codex-style sequential write without rollback (rejected) (count: 1)
- [whole-file-recovery] : Overwriting files after HUNK failure (forbidden) (count: 2)
- [mutate-rematch] : Rematching after each hunk mutation (fragile) (count: 2)

## 9) Next-Agent Brief

- **Read first:** `AGENTS.md`, `docs/contract-v1.md`, `docs/design/overview.md`, `docs/research-next-pass.md`; then `git status` / `git log --oneline`.
- **Ignore:** Treating `IMPLEMENTATION_PLAN.md` §20 “diffy backend” as current truth (superseded by design/stack); inventing Move/EOF without contract bump.
- **Try first:** (1) Commit dirty doc/AGENTS/next-pass if user wants clean tree; (2) refactor engine to locate→emit with CRLF tests mirroring Agents Python; or (3) add `*** End of File` behind contract update.
- **Success next turn:** Either clean commit of pending docs, or a green `cargo test` + `scripts/dogfood` after an engine/protocol change that matches design without introducing fuzzy default or non-transactional writes.

### Working tree note (important)

Uncommitted at handoff time:

- `M AGENTS.md`, `docs/architecture.md`, `docs/design/*`
- `?? docs/research-next-pass.md`

Do not assume they are on `main` until committed.
