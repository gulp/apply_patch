# Agent instructions

Operational guide for coding agents working in this repository.

## Localized file editing

Use the native exact-replacement edit for one small, unique string replacement.

Use `scripts/agent-patch` when:

- a change has multiple related hunks;
- several files must change atomically;
- additions or removals are clearer as contextual hunks;
- the user supplied a patch;
- exact replacement would require copying a large unchanged block.

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

Validate nontrivial patches first:

```bash
scripts/agent-patch --check < /tmp/change.patch
```

On failure: read the current affected region, regenerate the patch from current content, and retry. Do not recover by rewriting the entire file. Do not guess flags — run `scripts/agent-patch --help`.

Prefer patch **files** (or heredocs fed directly to the process) over `$(... <<'PATCH')` command substitution; bash warns and can truncate nested heredocs.

## Development commands

```bash
cargo build --release
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
scripts/test          # fmt check + clippy -D warnings + tests
scripts/lint          # fmt check + clippy
scripts/dogfood       # scenario gate (stale, ambiguous, path safety, add/delete)
scripts/agent-patch   # repo-local CLI wrapper (release → debug → cargo run)
```

`CLAUDE.md` is a symlink to this file.

## Working rules

- Prefer localized patches over whole-file rewrites.
- Keep changes scoped; do not add docs or refactors unrelated to the task.
- Do not commit unless the user asks.
- Public contract and protocol live under `docs/`; do not invent unsupported operations (no Move in v1).
- Match existing Rust module layout and error-code style when extending the tool.
- Frame documentation as current state; avoid “we added / now changed” narrative.
- Product overview belongs in `README.md`; keep this file operational only.

## Engine and dependency facts

- Apply path is a **custom exact matcher** + in-memory plan + transactional commit. **`diffy` is not the apply backend** (Codex/Zed use it for unified-diff *display*). Observational diffs use `similar`.
- Target apply shape: locate all chunks on the original lines, then emit with a forward cursor (`docs/design/apply-engine.md`). Prefer that over rematching a mutating buffer.
- Default matching is fail-closed unique exact. Do not add silent whitespace/Unicode fuzz or first-match-wins.
- On update, **file newline style wins** (preserve LF/CRLF); reject mixed endings. Fingerprints are BLAKE3.
- Primary-source caches: `opensrc path openai/codex#main`, `openai/openai-agents-python#main`, `openai/openai-agents-js#main`, `zed-industries/codex-acp#main`. npm `@openai/agents` resolves to `openai/openai-agents-js` (opensrc npm fetch may fail — use the GitHub repo).

## Avoid

- Assuming a Cargo/`package.json` dependency implies the apply algorithm (verify call sites).
- Partial filesystem writes before full in-memory validation; skipping rollback tests.
- Recovering from `HUNK_*` / stale failures by overwriting whole files.
- Guessing CLI flags or inventing protocol headers.
- `opensrc openai/agents` (wrong); use `openai/openai-agents-python` or `openai/openai-agents-js`.

## Reference (read when needed)

- [README.md](README.md) — product overview, CLI, protocol summary
- [docs/contract-v1.md](docs/contract-v1.md) — frozen semantics
- [docs/protocol.md](docs/protocol.md) — patch grammar
- [docs/errors.md](docs/errors.md) — error codes and exits
- [docs/design/](docs/design/) — architecture and engine design
- [docs/research-codex-apply-patch.md](docs/research-codex-apply-patch.md)
- [docs/research-openai-agents-apply-diff.md](docs/research-openai-agents-apply-diff.md)
