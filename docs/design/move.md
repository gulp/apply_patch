# Move File design (deferred)

`*** Move to:` / `*** Move File` stay **out of v1**. This note freezes commit-order rules for a future contract bump so implementation does not rediscover Codex/OpenClaw semantics under time pressure.

## Ground truth

| Source | Behavior |
| --- | --- |
| Codex | `*** Move to: dest` trailer on Update; scenarios `004_move_to_new_directory`, `010_move_overwrites_existing_destination` |
| Agents / OpenClaw / OpenCode | Same trailer form; OpenClaw also has path preflight extract |
| Our v1 | Parser rejects Move headers with `UnknownOperation` |

## Proposed v1.1 semantics (unique-exact + transactional)

1. **Grammar:** Prefer Codex trailer `*** Move to: <dest>` on an Update op (optional body hunks). Standalone `*** Move File: src` + `*** To: dest` remains unsupported unless we explicitly add it.
2. **Path policy:** Both src and dest must pass existing path rules (relative, no escape, no symlink escape). Dest parent dirs created transactionally like Add.
3. **Collision:**
   - Dest missing → allow.
   - Dest exists → **reject** (`FILE_ALREADY_EXISTS`) — deliberate delta vs Codex `010_*` overwrite.
4. **Empty/missing src:** Fail closed (`FILE_NOT_FOUND`); no partial write of dest.
5. **Commit order:** In-memory plan records `(src, dest, optional updated bytes)`. On commit: write dest (temp+rename) → delete src → on any failure roll back both (restore src if deleted; remove dest if created; restore overwritten dest only if we ever allow overwrite — we do not).
6. **Duplicate ops:** Same path as Add/Update/Delete target in one patch → reject.
7. **Check mode:** Validate + plan only; no FS mutation.
8. **JSON:** Report as `move` with `from` / `to` paths; blake3 of dest content after optional update body.

## Tests to port when enabling

- Codex `004_*` (success into new directory) — adapt expectations.
- Codex `010_*` — expect **failure** under our no-overwrite rule (document as intentional delta).
- Rollback: fail after dest write (inject I/O) → src intact, dest absent.
- `--check` move leaves tree unchanged.

## Non-goals until bump

Implementing Move in the parser/engine; inventing alternate headers; overwrite-dest.
