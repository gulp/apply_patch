# Research: OpenAI Agents Python `apply_diff` (V4A)

Primary source (via `opensrc path openai/openai-agents-python#main`):

- `~/.opensrc/repos/github.com/openai/openai-agents-python/main/src/agents/apply_diff.py`
- Envelope split: `src/agents/sandbox/capabilities/tools/apply_patch_tool.py`
- FS editor: `src/agents/sandbox/apply_patch.py`, `src/agents/editor.py`
- Tests: `tests/test_apply_diff.py`, `tests/test_apply_diff_helpers.py`
- Example: `examples/tools/apply_patch.py`

## Related: `@openai/agents` (npm)

`opensrc @openai/agents` failed with npm registry decode errors. Resolved via package metadata:

- npm: [`@openai/agents@0.13.5`](https://www.npmjs.com/package/@openai/agents)
- repo: `openai/openai-agents-js` (umbrella package re-exports `@openai/agents-core`)

Fetched with:

```bash
opensrc path openai/openai-agents-js#main
opensrc path openai/openai-agents-js@v0.13.5
```

Canonical implementation:

- `packages/agents-core/src/utils/applyDiff.ts` (~358 lines)
- Tests: `packages/agents-core/test/utils/applyDiff.test.ts` (rich examples 1–22)
- Export: `packages/agents-core/src/index.ts` → re-exported by `@openai/agents`
- Editor usage: `packages/agents-extensions/src/sandbox/shared/editor.ts`, `examples/tools/apply-patch.ts`

**JS vs Python:** same V4A algorithm (anchor seek → staged exact/rstrip/strip fuzz → chunk cursor apply). Difference: **Python preserves CRLF from the input file**; **JS always joins with `\n`** after `split('\n')`. Prefer the Python newline policy for `agent-patch`.

Note: `opensrc openai/agents` is not a GitHub repo; the package lives at `openai/openai-agents-python` (also cached as PyPI `openai-agents`).

## Architecture split (best practice)

Two layers, deliberately separated:

```text
*** Begin Patch envelope
        │
        ▼
parse_apply_patch_input  →  list[ApplyPatchOperation]
        │                     { type, path, diff, move_to? }
        ▼
WorkspaceEditor / ApplyPatchEditor
        │
        ├── create_file → apply_diff("", diff, mode="create")
        ├── update_file → apply_diff(original, diff, mode="default")
        └── delete_file → unlink
```

`apply_diff` is a **pure string → string** function. It never touches the filesystem. That matches our `engine/` vs `commit.rs` boundary and is worth keeping.

The model-facing envelope is still the Codex V4A grammar (`*** Begin Patch` / Add / Update / Delete / Move). Per-file bodies are sliced out and fed to `apply_diff` as the `diff` argument only (no file headers inside `apply_diff` for updates — just `@@` hunks and `+/-/ ` lines).

## `apply_diff` algorithm

### Modes

| Mode | Input | Diff shape | Behavior |
| --- | --- | --- | --- |
| `create` | ignored (`""`) | only `+` lines | Join contents; reject non-`+` lines |
| `default` | current file text | `@@` sections with context | Locate context, apply del/ins chunks |

### Newline handling (highly relevant)

1. Detect output newline from **input file** when updating (`CRLF` if present, else `LF`).
2. For create mode (or empty input), detect from the **diff** text.
3. Normalize input to LF for matching (`\r\n` → `\n`).
4. Re-emit with the detected newline via `newline.join(...)`.

Tests explicitly cover LF↔CRLF mismatches: **file newline style wins over patch newline style**. This is cleaner than rejecting mixed or forcing LF on Add — adopt for `agent-patch` update path (we already preserve file style; create currently forces LF).

### Update parse / match flow

1. Optional `@@ <anchor>` seeks a unique-ish class/function line; advances a **forward-only cursor**.
2. Bare `@@` allowed.
3. `_read_section` builds:
   - `next_context`: old-side lines (context + deletes) used for location
   - `section_chunks`: `(orig_index relative to context, del_lines, ins_lines)` groups split when returning to context after a change
4. `_find_context` from cursor (EOF prefers end-of-file first):
   - exact → fuzz `0`
   - `rstrip` → fuzz `1`
   - `strip` → fuzz `100`
   - EOF miss then fallback search adds fuzz `+10000`
5. First match wins (not unique-match). Fuzz is accumulated but not used to reject.
6. `_apply_chunks` walks chunks in order, copying unchanged regions, inserting `ins_lines`, skipping `del_lines`; **rejects overlapping / out-of-order chunks**.

### Chunk application model

```text
dest = orig[0:chunk.orig_index] + ins_lines + … + orig[cursor:]
cursor advances by len(del_lines)
```

This is equivalent to Codex Rust `compute_replacements` + `apply_replacements`. Applying on **original indices after all matches are resolved** (forward cursor emit) avoids coordinate remapping bugs from mutate-and-rematch.

## Comparison matrix

| Concern | Python `apply_diff` | Codex Rust `seek_sequence` | `agent-patch` v1 |
| --- | --- | --- | --- |
| Pure text apply API | Yes | Internal only | Yes (`apply_update`) |
| Unique match required | No (first hit) | No (first hit) | **Yes** |
| Whitespace fuzz | rstrip then strip | rstrip, strip, unicode normalize | No (exact + optional context reduction) |
| `@@` semantic anchor | Yes | Yes (`change_context`) | Unique exact cursor constraint |
| EOF marker | `*** End of File` | Yes | Yes (EOF-prefer exact, then unique forward) |
| Overlap detection | Yes (cursor check) | Sort + apply descending | Yes (`emit_chunks` forward cursor) |
| Apply shape | Locate on original → emit | `compute_replacements` → apply | `locate_chunks` → `emit_chunks` |
| CRLF preserve | File wins | Mostly LF-oriented split | File wins; reject mixed |
| Transactional multi-file | No (editor loops ops) | No | **Yes** |
| Create mode | Explicit `mode="create"` | AddFile path | Add File op |

## Alignment with `agent-patch`

1. **Layering:** envelope parse → per-file ops → pure `apply_update` → commit.
2. **Chunk apply with forward cursor** on the original line list after all matches are known (`locate_chunks` / `emit_chunks`).
3. **Newline policy from Python tests:** updates restore the file’s newline; patch CRLF/LF does not rewrite file style.
4. **Create mode** accepts only `+` lines (`Add File`).
5. **EOF preference** for `*** End of File` (exact end-aligned match first; unique forward fallback; no silent fuzz).
6. **No silent fuzz by default** — staged fuzz levels (`0 / 1 / 100`) remain a template only if an explicit `--fuzzy` mode appears later, still requiring uniqueness.
7. **Editor protocol** (`create_file` / `update_file` / `delete_file`) maps to `PlannedChange`; Responses API `ApplyPatchCall` uses headerless per-file `diff` bodies (see [research-next-pass.md](./research-next-pass.md)).

## What not to copy blindly

- First-match-wins under ambiguity (violates unique-exact).
- Accumulating fuzz without surfacing it to the caller (stable error codes).
- Non-transactional sequential writes in `WorkspaceEditor.apply_patch`.
- Allowing absolute paths in the example editor’s resolve path (sandbox path policy is stricter).
