# Agent instructions

Operational guide for coding agents in this repository.

## Localized edits

Native exact-replacement for one small unique string change.

Use `scripts/agent-patch` for multi-hunk or multi-file atomic edits, contextual add/remove, EOF-prefer updates, or user-supplied patches. Cursor skill: [`.cursor/skills/agent-patch/SKILL.md`](.cursor/skills/agent-patch/SKILL.md).

`agent-patch` is **not** on `PATH` — always invoke `scripts/agent-patch` from the repo root (release → debug → `cargo run`).

```bash
scripts/agent-patch <<'PATCH'
*** Begin Patch
*** Update File: path/to/file
@@
-old text
+new text
*** End Patch
PATCH
```

```bash
scripts/agent-patch --check < /tmp/change.patch   # validate, no writes
scripts/agent-patch --help                        # do not guess flags
```

On `HUNK_*` / stale failure: read current region, regenerate from current content, retry. Never whole-file overwrite as recovery.

Prefer a patch file (or a heredoc as the process stdin). Do not nest heredocs inside `$(...)` — bash truncates them.

`CLAUDE.md` → this file.

## Commands

```bash
cargo build --release
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
scripts/test          # fmt --check + clippy -D warnings + tests
scripts/lint
scripts/dogfood       # stale / ambiguous / path safety / add-delete / EOF gate
scripts/bench         # Criterion benches
scripts/agent-patch   # release → debug → cargo run
```

Fuzz (nightly + `cargo-fuzz`; see [fuzz/README.md](fuzz/README.md)):

```bash
cargo +nightly fuzz run parse_patch -- -max_total_time=60
cargo +nightly fuzz run path_policy -- -max_total_time=60
cargo +nightly fuzz run apply_update -- -max_total_time=60
```

## Rules

- Scope changes tightly; no drive-by refactors or unsolicited docs.
- Commit only when asked.
- Honor `docs/contract-v1.md` — no Move until the contract says so.
- Match existing module and `ErrorCode` style.
- Docs describe current state (no “we added / now changed” voice).
- Product docs → `README.md`; this file stays operational.

## Engine facts

- Apply = `locate_chunks` + `emit_chunks` (unique-exact, forward cursor) + in-memory plan + transactional commit. Observational diffs: `similar`.
- Modules: `engine/{matcher,locate,emit,apply,diff_summary}.rs`.
- **`diffy` / `flickzeug` are not V4A apply backends** (unified-diff display or fuzzy unified apply elsewhere).
- Update: file LF/CRLF wins; reject mixed. `*** End of File` → EOF-prefer exact locate. Fingerprints: BLAKE3.
- Codex-compatible fixtures: `crates/agent-patch/tests/fixtures/codex-scenarios/` (unique-exact subset).

## Avoid

- Inferring algorithms from dependency names — check call sites.
- Writing the tree before full in-memory validation; skipping rollback tests.
- Whole-file rewrite after patch failure.
- Inventing protocol headers or CLI flags.
- `opensrc openai/agents` → use `openai/openai-agents-python` or `openai/openai-agents-js`.
- `diffy::apply` / `flickzeug::apply` on `*** Begin Patch` text.

## Code reference search tools

Primary sources only.

### opensrc

```bash
opensrc path openai/codex#main
opensrc path openai/openai-agents-python#main
opensrc path openai/openai-agents-js#main
opensrc path zed-industries/codex-acp#main
opensrc path Aider-AI/aider#main
opensrc path openclaw/openclaw#main
opensrc path crates:flickzeug
opensrc path crates:similar
opensrc list
```

Stderr = progress; stdout = path (`$(opensrc path …)` is fine). Cache: `~/.opensrc/repos/…`. If npm `@scope/name` fails, `npm view … repository` then opensrc the GitHub repo. Always `rg`/Read call sites after fetch.

### grep-app MCP (`searchGitHub`)

Literal code (or `useRegexp`), not English. Examples: `"*** End of File"`, `seek_sequence(`, `FuzzyConfig`, `apply_diff(`. Narrow with `repo` / `language`. Discover callers → `opensrc path` → read full files.

### Workflow

1. grep-app markers/APIs → repos/paths  
2. `opensrc path owner/repo#main` or `crates:name`  
3. Update `docs/research-*.md` / `docs/design/` when semantics matter  
4. Active plan: [IMPLEMENTATION_PLAN.md](IMPLEMENTATION_PLAN.md) · backlog: [docs/research-next-pass.md](docs/research-next-pass.md)

## Reference

- [README.md](README.md) — CLI, protocol summary  
- [docs/contract-v1.md](docs/contract-v1.md) · [docs/protocol.md](docs/protocol.md) · [docs/errors.md](docs/errors.md)  
- [IMPLEMENTATION_PLAN.md](IMPLEMENTATION_PLAN.md) — active post-v1 plan  
- [docs/design/](docs/design/) · [docs/research-codex-apply-patch.md](docs/research-codex-apply-patch.md) · [docs/research-openai-agents-apply-diff.md](docs/research-openai-agents-apply-diff.md) · [docs/research-next-pass.md](docs/research-next-pass.md)
