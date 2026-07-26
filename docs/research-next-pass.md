# Next-pass backlog

Fact-backed follow-ups (not v1 contract changes). Sources via opensrc + grep-app.

## Protocol gaps (in upstream V4A, deferred here)

| Item | Ground truth | Work |
| --- | --- | --- |
| `*** End of File` | Codex grammar; Agents `END_FILE` + EOF-prefer locate; Aider `find_context(..., eof=True)` | Parse flag; exact locate prefer `len-context` then forward |
| `*** Move to:` | Trailer on Update in Codex/Agents/OpenClaw/OpenCode | Collision rules; write dest → delete src; rollback both; Codex `004_move_*` |

## Engine

| Item | Ground truth | Work |
| --- | --- | --- |
| Locate-all → emit | Agents `_apply_chunks`; Codex `compute_replacements` | Done: `engine/locate.rs` + `engine/emit.rs` behind `apply_update` |
| CRLF matrix | Agents Python tests; JS always `\n` | Done in `engine/apply` unit tests (file newline wins) |
| Exact Codex scenarios | `codex-rs/apply-patch/tests/fixtures/scenarios/` | Port subset that matches unique-exact + no overwrite-Add |
| `@@ <anchor>` unique locate | Agents/Codex change_context; our contract unique-exact | Done: stored on `Hunk.anchor`; numeric `@@ -l,s…` ignored |

## Optional harness helpers

| Item | Ground truth | Work |
| --- | --- | --- |
| Path preflight list | OpenClaw `extractApplyPatchTargetPaths` | Library helper or `--list-paths` |
| Explicit `--fuzzy` | Codex/Agents/Aider fuzz ladders; flickzeug `FuzzyConfig` is **unified-diff** only | If ever: unique match at chosen fuzz level; never default |

## Still unprobed

- OpenAI Responses `ApplyPatchCall` JSON schema (`openai-node` / `openai-python`)
- Already-applied detection (flickzeug `is_diff_applied*`; Codex `delta.exact`)
- Codex `streaming_parser.rs` for huge patches

## Tool notes

| Tool | Durable rule |
| --- | --- |
| opensrc | Prefer `owner/repo#main` or `crates:name`; npm → GitHub if decode fails |
| grep-app | Literal code; always pass MCP `server` + `toolName` |
| Wrong | `opensrc openai/agents`; V4A text into `diffy`/`flickzeug` apply |
