# agent-patch v1 Contract

Frozen decisions for parallel implementation. Changing any field requires an explicit contract bump.

## Operations

Supported: `Add File`, `Update File`, `Delete File`.

Deferred to v1.1: `Move File`.

## Matching

1. Exact full hunk-context match (unique).
2. `@@` / `@@ <anchor>` constrains the search cursor: bare `@@` is a section break; `@@ <text>` requires a unique exact line match for `<text>` before locating the hunk body. Unified-diff numeric `@@ -l,s +l,s @@` markers are ignored as location math.
3. `*** End of File` on a hunk: prefer an exact match aligned at EOF (`len - old_len`); if that fails, fall back to unique-exact forward search from the cursor (no whitespace/unicode fuzz).
4. Controlled edge-context reduction: strip one leading context line, then one trailing, stop at minimum of one remaining old line; accept only if unique.
5. Zero matches → `HUNK_NOT_FOUND`.
6. Multiple matches → `HUNK_AMBIGUOUS`.

No fuzzy, whitespace-normalized, or first-match-wins behavior.

Apply algorithm: locate all chunks on the original line array, then emit with a forward cursor (OpenAI Agents `apply_diff` shape). Do not rematch against a mutating buffer.

## Line endings / encoding

- Preserve LF and CRLF on update (file newline style wins; patch CRLF/LF does not rewrite the file style).
- Reject mixed line endings on update.
- Preserve UTF-8 BOM when present.
- UTF-8 text only; binary unsupported.
- Add File content uses LF (join `+` lines with `\n`).

## Backend

Custom exact locator + cursor emit. Not `diffy`. `similar` is observational only.

## Semantics

| Topic | Decision |
| --- | --- |
| Delete confirmation | Path-state only (exists + regular file) |
| Empty-file deletion | Allowed |
| No-op patch / no-op update | Fail (`PATCH_NO_EFFECT`) |
| Executable add mode | Deferred |
| Rollback storage | In-memory under file-size limit |
| Directory creation for Add | Implicit and transactional (created parents tracked for rollback) |
| Absolute paths in JSON | Root shown as resolved absolute path; file paths remain repo-relative |
| Hash | BLAKE3; labeled `blake3` in JSON |
| fsync | Enabled for temp files by default |
| Context reduction | Supported when unique and deterministic |

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | success |
| 1 | patch does not apply |
| 2 | malformed / unsupported patch |
| 3 | filesystem / I/O failure |
| 4 | unsafe path / policy |
| 5 | concurrent modification |
| 6 | internal invariant / rollback failed |
| 7 | resource limit exceeded |

## Default limits

```text
max_patch_bytes      4 MiB
max_file_bytes       16 MiB
max_files            128
max_hunks_per_file   256
max_total_hunks      2,048
```
