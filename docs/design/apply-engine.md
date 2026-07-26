# Apply engine

Pure text in → pure text out. No paths, FS, or JSON.

Grounded in Agents `apply_diff` / `applyDiff` and Codex `compute_replacements`, with unique-exact matching and Python-style newline preservation.

## Seam

```rust
fn apply_update(
    base: &str,
    hunks: &[Hunk],
    newline: Newline,      // from snapshot: Lf | CrLf
    final_newline: bool,
    bom: BomPolicy,
) -> Result<AppliedText, ApplyError>;
```

Planner supplies newline/BOM from the snapshot. Engine never reads the disk.

## Algorithm: locate then emit

Do not rematch a mutating buffer. Agents (`_apply_chunks`) and Codex (`compute_replacements` → `apply_replacements`) resolve positions on the **original** line list, then emit.

```text
Phase A — Locate (original lines, forward cursor)
  for each hunk:
      if @@ <anchor>: unique exact line for <anchor> at/after cursor, else fail
      if *** End of File: prefer exact match at EOF, else unique forward search
      candidates ← exact old-side matches in window
      optional context reduction → accept only if unique
      0 → HUNK_NOT_FOUND; >1 → HUNK_AMBIGUOUS
      record Chunk { orig_index, del_len, ins_lines }; advance cursor

Phase B — Emit
  for chunk in ascending orig_index:
      if cursor > orig_index → HUNK_OVERLAP
      copy orig[cursor..orig_index]; append ins_lines; cursor += del_len
  copy tail; join with snapshot newline; restore final_newline + BOM
  if result == base → PATCH_NO_EFFECT
```

## Old-side / new-side

| Line | Old | New |
| --- | --- | --- |
| ` context` | ✓ | ✓ |
| `-old` | ✓ | |
| `+new` | | ✓ |

`del_len = old_side.len()`, `ins_lines = new_side`.

Pure `+` with context: old-side is context; match uniquely. Pure `+` with no context on a non-empty file → `HUNK_AMBIGUOUS` (Agents empty context appends at cursor — we do not). New file content uses `Add File` / create path, not Update.

## `@@` anchors

- Bare `@@`: section break only.
- `@@ <text>`: unique exact line `<text>` at/after cursor; then body match. Fail closed if 0 or >1.
- `@@ -l,s +l,s @@`: ignore as line-number math; body still exact. (Agents may treat the whole marker string as an anchor — we do not.)

## Context reduction (contract)

After exact full old-side is 0 or ambiguous: strip one leading context, then one trailing; never strip deletes; accept only if exactly one match remains. No edit-distance or whitespace normalization.

## Newlines and BOM

```text
snapshot: CrLf if "\r\n" present without bare LF mix; else Lf; Mixed → reject on Update
match: strip \r from logical lines (same idea as Aider _norm / Agents normalize)
emit: join with snapshot newline; preserve final_newline; preserve leading U+FEFF
Add File: join +lines with \n
```

JS Agents `applyDiff` always joins with `\n`. `agent-patch` follows the Python Agents policy: file newline wins on update.

## Create path

Outside `apply_update`: `join(plus_lines, "\n")` (+ trailing newline when non-empty). Same as Agents `mode="create"`.

## Errors

| Condition | Code |
| --- | --- |
| No / many body matches | `HUNK_NOT_FOUND` / `HUNK_AMBIGUOUS` |
| Bad / many anchors | same (+ `@@` hint) |
| Overlap | `HUNK_OVERLAP` |
| No byte change | `PATCH_NO_EFFECT` |

## Non-features (v1)

- `diffy::apply` / `flickzeug::apply`
- Default rstrip/strip/unicode fuzz (Codex `seek_sequence` / Agents fuzz ladder)
- First-match-wins
- `*** Move to:` (see [`move.md`](move.md))

`*** End of File`: prefer exact match at `len - old_len`, else unique forward search. Pure `+` insertion with `*** End of File` appends at EOF.

## Crate map

| File | Role |
| --- | --- |
| `engine/matcher.rs` | Unique exact match + context reduction (`find_unique_match`) |
| `engine/locate.rs` | Phase A: `locate_chunks` on original lines (forward cursor, `@@` anchors, EOF) |
| `engine/emit.rs` | Phase B: `emit_chunks` forward cursor join |
| `engine/apply.rs` | `apply_update` orchestration, split/join, newline detect |
| `engine/diff_summary.rs` | `similar` line counts |

## Tests

- Unit: exact / ambiguous / not-found, `@@` anchors, EOF-prefer, CRLF file-wins matrix, multi-hunk locate→emit
- Integration: CLI, atomicity, concurrency, path safety, limits
- Codex portable subset: `tests/fixtures/codex-scenarios/` + `tests/codex_scenarios.rs`
- Dogfood: `scripts/dogfood` (includes EOF)
- Fuzz: `fuzz/fuzz_targets/{parse_patch,path_policy,apply_update}.rs`
- Bench: `benches/apply_update.rs`