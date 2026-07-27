# Error Codes

Public diagnostics map each failure to a stable code and exit class. JSON mode places them under `error.code` / `error.exit_code`. See [contract-v1.md](./contract-v1.md) and [contract-v2.md](./contract-v2.md).

| Code | Exit | Meaning |
| --- | --- | --- |
| `PATCH_MISSING_BEGIN` | 2 | Missing `*** Begin Patch` |
| `PATCH_MISSING_END` | 2 | Missing `*** End Patch` |
| `PATCH_EMPTY` | 2 | No operations |
| `PATCH_TRAILING_CONTENT` | 2 | Content outside envelope |
| `UNKNOWN_OPERATION` | 2 | Unrecognized header |
| `MALFORMED_HUNK` | 2 | Invalid hunk syntax |
| `DUPLICATE_PATH` | 2 | Two operations target the same path |
| `INVALID_PATH` | 4 | Empty, absolute, `..`, NUL, etc. |
| `PATH_OUTSIDE_ROOT` | 4 | Escapes configured root |
| `SYMLINK_ESCAPE` | 4 | Symlink leads outside root |
| `PATH_COLLISION` | 4 | Distinct paths alias same object |
| `FILE_ALREADY_EXISTS` | 1 | Add targets existing path |
| `FILE_NOT_FOUND` | 1 | Update/Delete target missing |
| `NOT_REGULAR_FILE` | 4 | Directory, symlink, special file |
| `BINARY_FILE_UNSUPPORTED` | 2 | Binary content |
| `INVALID_UTF8` | 2 | Non-UTF-8 text |
| `MIXED_LINE_ENDINGS` | 2 | Mixed LF/CRLF on update |
| `HUNK_NOT_FOUND` | 1 | No unique match |
| `HUNK_AMBIGUOUS` | 1 | Multiple matches |
| `HUNK_OVERLAP` | 1 | Overlapping hunk effects |
| `PATCH_NO_EFFECT` | 1 | Applied result identical to base |
| `CONCURRENT_MODIFICATION` | 5 | File changed before commit |
| `ATOMIC_COMMIT_FAILED` | 3 | Commit I/O failure |
| `ROLLBACK_FAILED` | 6 | Rollback incomplete |
| `LIMIT_PATCH_BYTES` | 7 | Patch too large |
| `LIMIT_FILE_BYTES` | 7 | File too large |
| `LIMIT_FILE_COUNT` | 7 | Too many files |
| `LIMIT_HUNK_COUNT` | 7 | Too many hunks |
| `IO_ERROR` | 3 | Generic I/O |
| `INTERNAL_ERROR` | 6 | Invariant violation |
| `INPUT_ERROR` | 2 | Cannot read patch input |

Post-v1 / extended codes (see [contract-v2.md](./contract-v2.md)):

| Code | Exit | Meaning |
| --- | --- | --- |
| `HASH_PIN_MISMATCH` | 5 | Content-hash pin ≠ snapshot |
| `ROOT_LOCKED` | 5 | Another writer holds `.agent-patch/lock` |
| `RECOVERY_REQUIRED` | 6 | Incomplete journal; run `recover` |
| `RECOVERY_AMBIGUOUS` | 6 | Cannot prove all-before or all-after |
| `VERIFY_FAILED` | 1 | Verify command non-zero; root unchanged |
| `VERIFY_TIMEOUT` | 1 | Verify exceeded wall clock |
| `VERIFY_SIGNALLED` | 1 | Verify killed by signal |
| `RISK_REFUSED` | 1 | Risk gate refused the match |
| `PARTIALLY_APPLIED` | 1 | Idempotent mode: incompatible partial replay |
| `RECEIPT_INVALID` | 2 | Bad or unsupported receipt |
| `RECEIPT_OBJECT_MISSING` | 6 | Referenced before-image object missing |
| `REVERT_STALE` | 5 | Tree ≠ receipt after-hashes |
| `SHADOW_LIMIT_EXCEEDED` | 7 | Shadow files/bytes/time budget |
| `MATCH_WORK_LIMIT` | 7 | Matcher work budget exceeded |

`ALREADY_APPLIED` is a success status field (exit 0), not an error code.

Human mode prints `error[CODE]: …` plus optional `path`, indices, source span, and a next-action `hint` on stderr. JSON mode emits a single object on stdout and keeps stderr empty for structured failures.
