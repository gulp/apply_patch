# Transaction

Validated plan → tree mutation or clean failure. Deliberate break from Codex/Agents sequential disk writes.

## Guarantee

All ops are validated and applied in memory before any visible mutation. Per-file replace uses same-directory temp + atomic rename where the OS allows. If commit fails after a visible mutation, committed ops roll back from in-memory bytes. Multi-file atomic visibility is not an FS primitive; partial commits are undone and reported — never claim DB-style atomicity.

## Phases

```text
1. Plan         PatchPlan { entries, base_fingerprints }
2. Revalidate   existence + blake3 vs snapshot
3. Prepare      temps for create/update; track created parents
4. Commit       lexicographic path order (v1; no Move)
5. Rollback     if step 4 fails mid-way
6. Cleanup      leftover temps; empty created dirs on rollback
```

`--check`: stop after in-memory plan/apply. Zero temps/writes (`CountingFs`).

## Identity

```rust
struct BaseIdentity {
    exists: bool,
    content: Option<ContentFingerprint>, // blake3 of exact bytes
}
```

Content hash is authoritative. Concurrent modification is not retried; caller regenerates the patch.

## Actions

| Entry | Commit | Rollback |
| --- | --- | --- |
| Modify | rename temp → path | restore bytes via temp+rename + mode |
| Create | mkdir parents; rename temp → path | unlink; rmdir empty created parents |
| Remove | unlink | restore bytes + mode |

When `*** Move to:` lands (v1.1): write dest then remove source; rollback both; cover collisions (Codex `004_move_*`, `010_move_overwrites_*`).

## Failures

| Case | Exit | Code |
| --- | --- | --- |
| Drift before commit | 5 | `CONCURRENT_MODIFICATION` |
| Commit I/O (after rollback attempt) | 3 | `ATOMIC_COMMIT_FAILED` |
| Incomplete rollback | 6 | `ROLLBACK_FAILED` (list paths) |

## Upstream contrast

| System | Behavior |
| --- | --- |
| Codex | Sequential FS apply; `AppliedPatchDelta`; `015_*leaves_changes` keeps earlier ops |
| Agents `WorkspaceEditor` | Per-op write immediately |
| `agent-patch` | No visible write until full plan; rollback on commit failure |

## Fault injection

All mutations via `FileSystem` (`fs.rs`): fail Nth rename/delete/flush/rollback. Expect clean tree or `ROLLBACK_FAILED`, no orphan temps on ordinary failures.
