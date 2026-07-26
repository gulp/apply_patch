# Transaction Design

Deep module: **validated plan in → tree mutation or clean failure**. Differentiator vs OpenAI apply paths.

## Guarantee (honest)

> All operations are validated and applied in memory before any visible mutation. Per-file replacement uses same-directory temp + atomic rename where the OS allows. If a commit step fails after visible mutation begins, already-committed ops are rolled back from in-memory material. Multi-file atomic visibility is not a filesystem primitive; partial commits are actively undone and reported.

Never claim database-style multi-file atomicity.

## Phases

```text
1. Plan          PatchPlan { entries, base_fingerprints }
2. Revalidate    every path still matches base identity
3. Prepare       temps for creates/updates; track created parents
4. Commit        deterministic order: updates → adds → deletes
                 (lexicographic path within each class, or single lex order)
5. Rollback      on failure after step 4 started
6. Cleanup       remove leftover temps; remove empty created dirs on rollback
```

`--check` stops after step 1 (and in-memory apply). Zero temps, zero writes.

## Identity

```rust
struct BaseIdentity {
    exists: bool,
    content: Option<ContentFingerprint>, // blake3 of exact bytes
}
```

Revalidation compares existence + hash. Metadata (mtime/inode) may be used as a cheap precheck later; content hash remains authoritative.

## Commit actions

| Plan entry | Visible mutation | Rollback |
| --- | --- | --- |
| Modify | rename temp → path | rewrite original bytes via temp+rename |
| Create | rename temp → path (after mkdir parents) | unlink; rmdir created parents (empty) |
| Remove | unlink | restore bytes via temp+rename + mode |

## Ordering rationale

Without Move support, **lexicographic path order** is enough (contract). Prefer grouping updates before adds before deletes only if a future Move feature needs collision unblocking; document any change as a contract bump.

## Failure taxonomy

| Failure | Exit | Code |
| --- | --- | --- |
| Hash drift / type change before commit | 5 | `CONCURRENT_MODIFICATION` |
| Temp/write/rename/unlink I/O | 3 | `ATOMIC_COMMIT_FAILED` (after rollback attempt) |
| Rollback incomplete | 6 | `ROLLBACK_FAILED` (list unrestored paths) |

Concurrent modification is **not** retried internally. Caller rereads and regenerates.

## What OpenAI does differently

Codex applies hunks sequentially to disk and records `AppliedPatchDelta`; scenario `015` expects partial success to remain. Agents `WorkspaceEditor.apply_patch` loops operations with immediate writes.

We intentionally diverge: agents that need a clean tree on failure should use `agent-patch`.

## Fault-injection seam

All mutations go through `FileSystem` trait (`fs.rs`). Integration tests inject:

- fail Nth rename
- fail delete
- fail temp flush
- fail rollback write

Assert: no partial intended state; no orphan temps after ordinary failures; `ROLLBACK_FAILED` lists paths when restore fails.

## Check-mode invariant

`CountingFs` (or equivalent) asserts zero `create_temp` / `rename` / `remove` / `create_dir_all` / `write_temp` during `--check`.
