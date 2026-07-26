# Next-pass backlog

Remaining fact-backed follow-ups. Implemented capabilities live in the contract, protocol, and design docs. Sources via opensrc + grep-app.

## Protocol / product

| Item | Ground truth | Work |
| --- | --- | --- |
| `*** Move to:` | Trailer on Update in Codex/Agents/OpenClaw/OpenCode | Implement per [`design/move.md`](design/move.md); no overwrite-dest (delta vs Codex `010_*`) |
| Path preflight list | OpenClaw `extractApplyPatchTargetPaths` | Library helper or `--list-paths` |
| Explicit `--fuzzy` | Codex/Agents/Aider fuzz ladders; flickzeug `FuzzyConfig` is **unified-diff** only | Unique match at chosen fuzz level; never default |
| Streaming patch parse | Codex `streaming_parser.rs` | Only if harnesses stream past `max_patch_bytes` before `*** End Patch` |
| Responses op bridge | `ApplyPatchCall` / `create_file`\|`update_file`\|`delete_file` with headerless `diff` | Optional `--operation-json`; CLI stays envelope-oriented |
| Stronger already-applied hint | flickzeug reverse-round-trip (unified only); Codex `delta.exact` is FS-delta trust | Optional distinct hint when new side present / old side absent — still fail-closed, not silent success |

## Reference notes (probed)

### Responses API `ApplyPatchCall`

- `openai/openai-python` `src/openai/types/responses/response_apply_patch_tool_call.py`
- `type: "apply_patch_call"`; operations: `create_file` / `update_file` (`path` + headerless `diff`), `delete_file` (`path`)
- Agents JS: `ApplyPatchCallItem` in `packages/agents-core/src/types/protocol.ts`

### Already-applied vs no-effect

| System | Mechanism |
| --- | --- |
| flickzeug | `is_diff_applied_with_config` — unified-diff reverse round-trip |
| Codex | `delta.exact` — whether FS delta remains trustworthy after write/delete failures |
| agent-patch | `PATCH_NO_EFFECT` when update emit equals base |

### Tool notes

| Tool | Durable rule |
| --- | --- |
| opensrc | Prefer `owner/repo#main` or `crates:name`; npm → GitHub if decode fails |
| grep-app | Literal code; always pass MCP `server` + `toolName` |
| Wrong | `opensrc openai/agents`; V4A text into `diffy`/`flickzeug` apply |
