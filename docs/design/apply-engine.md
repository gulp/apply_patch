# Apply Engine Design

Deep module: **pure text in → pure text out**. No paths, no FS, no JSON.

Inspired by OpenAI Agents `apply_diff` / `applyDiff`, adapted to `agent-patch` uniqueness and newline rules.

## External seam

```rust
/// Apply validated update hunks to a snapshot of file text.
///
/// - `newline` is the *output* line ending taken from the snapshot (LF or CRLF).
/// - Matching always uses LF-normalized logical lines.
/// - Returns Err on not-found, ambiguous, overlap, or no-effect.
fn apply_update(
    base: &str,
    hunks: &[Hunk],
    newline: Newline,
    final_newline: bool,
    bom: BomPolicy,
) -> Result<AppliedText, ApplyError>;
```

Callers (planner) supply snapshot-derived newline/BOM. The engine never inspects the filesystem.

## Two phases (the elegant core)

Do **not** rematch against a mutating buffer. Do what Agents/Codex do:

```text
Phase A — Locate (on original lines only)
  for each hunk in source order:
      window ← after previous match end (forward cursor)
      if @@ anchor present: advance window start to unique exact anchor (or fail)
      candidates ← exact matches of hunk old-side within window…EOF
          (optional: controlled context reduction if exactly one remains)
      if 0 → HUNK_NOT_FOUND
      if >1 → HUNK_AMBIGUOUS
      record Chunk { orig_index, del_len, ins_lines }
      advance cursor past this match

Phase B — Emit (forward cursor, original array)
  dest = []
  cursor = 0
  for chunk in chunks (ascending orig_index):
      if cursor > chunk.orig_index → HUNK_OVERLAP
      dest.extend(orig[cursor .. orig_index])
      dest.extend(ins_lines)
      cursor = orig_index + del_len
  dest.extend(orig[cursor ..])
  join with snapshot newline; restore final_newline + BOM
  if dest == base → PATCH_NO_EFFECT
```

This gives:

- **Locality** — matching bugs live in Phase A; emit bugs in Phase B.
- **Overlap detection** — trivial (`cursor > orig_index`).
- **Determinism** — no order-dependent re-search after edits.
- **Testability** — Phase A and B unit-testable without fixtures on disk.

## Hunk old-side / new-side

Same as today:

| Patch line | Old-side | New-side |
| --- | --- | --- |
| ` context` | yes | yes |
| `-old` | yes | no |
| `+new` | no | yes |

`del_len = old_side.len()`, `ins_lines = new_side` (owned strings).

Pure insertion (no `-` lines, only `+` and optional context):

- With leading/trailing context: old-side is the context lines; match uniquely; insert at the change point (Agents section-chunk model).
- With **no** context on non-empty file: **fail** `HUNK_AMBIGUOUS` (unlike Agents empty-context → append at cursor). Empty file may accept pure create-style insertion only via `Add File`, not Update.

## `@@` anchor semantics (v1 refinement)

Treat `@@` / `@@ <text>` as a **cursor constraint**, not as fuzzy equality:

1. Bare `@@` — no anchor advance.
2. `@@ <text>` — find **unique exact** line equal to `<text>` at or after current cursor.
   - 0 → `HUNK_NOT_FOUND` (anchor)
   - >1 → `HUNK_AMBIGUOUS` (anchor)
   - 1 → set cursor to line after anchor, then match the hunk body
3. Unified-diff style `@@ -l,s +l,s @@` — **ignored as location math** (advisory); body matching still exact. (Agents currently treat the whole marker string as an anchor line — we do not; that surprises on real files.)

This preserves agent prompts that teach `@@ class Foo` without importing first-match fuzz.

## Context reduction (unchanged contract)

Only after exact full old-side yields 0 or >1 matches:

- Strip one leading context line, then one trailing, iteratively.
- Never strip delete lines.
- Accept only if **exactly one** match remains.
- Never use edit-distance or whitespace normalization.

## Newline and BOM

```text
detect snapshot newline: CRLF if "\r\n" in raw bytes (and no bare LF), else LF, else None
reject Mixed on Update
normalize logical lines: strip \r from ends for matching
emit: join(logical_lines, snapshot_newline)
final_newline: preserve whether base ended with newline
BOM: if base started with U+FEFF, ensure output does too
```

**Create (`Add File`):** join `+` payloads with `\n` (protocol lines are LF-oriented). v1.1 may add explicit CRLF create if needed.

## Create path

Not part of `apply_update`. Planner handles Add:

```text
content = join(plus_lines, "\n") + trailing "\n" if any lines
```

Mirrors Agents `mode="create"` without overloading the update engine.

## Error model (engine-local)

| Condition | Code |
| --- | --- |
| No unique body match | `HUNK_NOT_FOUND` / `HUNK_AMBIGUOUS` |
| Bad/ambiguous anchor | same, with hint mentioning `@@` |
| Overlapping chunks | `HUNK_OVERLAP` |
| Identity after emit | `PATCH_NO_EFFECT` |

Engine returns typed errors; diagnostics layer attaches path, indices, spans, hints.

## Explicit non-features

- No `diffy::apply`
- No rstrip/strip/unicode passes in v1
- No first-match-wins
- No reading files

## Test plan for this module

1. Port Agents examples 1–22 that don’t rely on fuzz (exact cases).
2. Ambiguous repeated blocks → `HUNK_AMBIGUOUS`.
3. CRLF file + LF patch → CRLF output (Python tests).
4. Overlapping hunks → `HUNK_OVERLAP`.
5. `@@` unique anchor + body match; duplicate anchors fail.
6. Property: apply is deterministic; no panic on random hunk structs within limits.

## Mapping in the crate

| Module | Responsibility |
| --- | --- |
| `engine/matcher.rs` | Phase A locator (`find_unique_match`) |
| `engine/apply.rs` | Apply orchestration and line join/split |
| `engine/diff_summary.rs` | Observational line counts via `similar` |

Target structure for the locate-all → cursor-emit refactor: `engine/locate.rs` + `engine/emit.rs` behind the same `apply_update` seam.
