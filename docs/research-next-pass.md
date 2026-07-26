# Next-pass backlog

Fact-backed follow-ups. Sources via opensrc + grep-app.

## Protocol gaps

| Item | Ground truth | Status |
| --- | --- | --- |
| `*** End of File` | Codex grammar; Agents `END_FILE` + EOF-prefer locate; Aider `find_context(..., eof=True)` | **Done** — parse flag; prefer exact at `len-old_len`, else unique forward |
| `*** Move to:` | Trailer on Update in Codex/Agents/OpenClaw/OpenCode | **Designed, deferred** — [`design/move.md`](design/move.md); no overwrite-dest (delta vs Codex `010_*`) |

## Engine

| Item | Ground truth | Status |
| --- | --- | --- |
| Locate-all → emit | Agents `_apply_chunks`; Codex `compute_replacements` | **Done** — `engine/locate.rs` + `engine/emit.rs` |
| CRLF matrix | Agents Python tests; JS always `\n` | **Done** — file newline wins |
| Exact Codex scenarios | `codex-rs/apply-patch/tests/fixtures/scenarios/` | **Done** — compatible subset under `tests/fixtures/codex-scenarios/` |
| `@@ <anchor>` unique locate | Agents/Codex change_context; our contract unique-exact | **Done** |

## Optional harness helpers

| Item | Ground truth | Work |
| --- | --- | --- |
| Path preflight list | OpenClaw `extractApplyPatchTargetPaths` | Library helper or `--list-paths` |
| Explicit `--fuzzy` | Codex/Agents/Aider fuzz ladders; flickzeug `FuzzyConfig` is **unified-diff** only | If ever: unique match at chosen fuzz level; never default |

## Probed (was unprobed)

### Responses API `ApplyPatchCall`

- **Where:** `openai/openai-python` `src/openai/types/responses/response_apply_patch_tool_call.py` (Stainless-generated from OpenAPI).
- **Shape:** `type: "apply_patch_call"` with `operation` discriminated union:
  - `create_file` / `update_file` → `{ type, path, diff }` (`diff` is headerless per-file body, not full `*** Begin Patch` envelope)
  - `delete_file` → `{ type, path }`
- **Also:** Agents JS `ApplyPatchCallItem` in `packages/agents-core/src/types/protocol.ts`.
- **Implication for us:** CLI remains envelope-oriented; a future `--operation-json` bridge could accept Responses ops and wrap/unwrap. Not required for v1.

### Already-applied detection

| System | Mechanism | Relevance |
| --- | --- | --- |
| flickzeug | `is_diff_applied_with_config`: reverse unified-diff, re-apply forward, compare | Unified-diff only — **not** V4A |
| Codex | `delta.exact` on applied patch change list | Tracks whether FS delta is trustworthy after write/delete failures — **not** “patch already applied” |
| agent-patch | `PATCH_NO_EFFECT` when update emit equals base | Closest analogue; fail-closed rather than “AlreadyApplied” success |

**Decision:** Do not port flickzeug already-applied. Optional future: detect “new side already present + old side absent” as a distinct hint without succeeding silently.

### Codex `streaming_parser.rs`

- Incremental line state machine (`NotStarted` → `StartedPatch` → Add/Update/Delete → `EndedPatch`).
- Handles `*** End of File`, `*** Move to:`, environment id markers mid-stream.
- **Our v1:** whole-buffer parse under `max_patch_bytes` (4 MiB). Streaming only needed if agents stream multi-MiB patches before End Patch — park until a concrete harness requires it.

## Tool notes

| Tool | Durable rule |
| --- | --- |
| opensrc | Prefer `owner/repo#main` or `crates:name`; npm → GitHub if decode fails |
| grep-app | Literal code; always pass MCP `server` + `toolName` |
| Wrong | `opensrc openai/agents`; V4A text into `diffy`/`flickzeug` apply |
