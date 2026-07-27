# Transaction

Validated plan → tree mutation or clean failure. Deliberate break from Codex/Agents sequential disk writes. Durable journals and CAS before-images: [transaction-journal.md](./transaction-journal.md). Contract: [contract-v2.md](../contract-v2.md).

## Guarantee

All ops are validated and applied in memory before any visible mutation. Before the first rename/delete, the coordinator acquires `.agent-patch/lock`, refuses incomplete journals, stores before-image objects for updates/deletes, and writes a durable `PREPARED` journal. Per-file replace uses same-directory temp + atomic rename where the OS allows. Mid-commit failure uses in-process rollback when possible; otherwise leaves a recoverable journal. Multi-file atomic visibility is not an FS primitive — the guarantee is durable recoverability to proven all-before or all-after.

## Phases

```text
1. Plan         PatchPlan { entries, base_fingerprints, plan_digest }
2. Lock         exclusive `.agent-patch/lock` (mutating paths only)
3. Gate         refuse incomplete journals (RECOVERY_REQUIRED)
4. Revalidate   existence + blake3 vs snapshot
5. Objects      put before-images for update/delete
6. Journal      PREPARED → COMMITTING
7. Commit       temps + rename/delete in plan order
8. Finalize     COMPLETED journal + internal receipt
9. Rollback     in-process if commit fails mid-way; else recover
```

`--check` / `--plan`: stop after in-memory plan. Zero temps/writes to the real tree.  
`--verify`: shadow + argv first; lock and journaled commit only on promote.

## Identity

```rust
struct BaseIdentity {
    exists: bool,
    content: Option<ContentFingerprint>, // blake3 of exact bytes
}
```

Content hash is authoritative. Concurrent modification is not retried; caller regenerates the patch. Optional `*** Hash: blake3 <hex>` pins fail before locate (`HASH_PIN_MISMATCH`).

## Actions

| Entry | Commit | Rollback / recover |
| --- | --- | --- |
| Modify | rename temp → path | restore from CAS object + mode |
| Create | mkdir parents; rename temp → path | unlink; rmdir empty created parents |
| Remove | unlink | restore from CAS object + mode |

When `*** Move to:` lands: write dest then remove source; rollback both; cover collisions (Codex `004_move_*`, `010_move_overwrites_*`).

## Receipts and revert

Successful mutation writes `.agent-patch/receipts/<txid>.json` referencing before-image objects (hashes-only receipts are invalid). Receipts record `permissions.mode` / `executable` for update/delete; revert restores those bits. `revert <RECEIPT>` proves current after-states, then runs a new journaled inverse transaction. `gc [--dry-run]` removes only unreferenced objects.

## Failures

| Case | Exit | Code |
| --- | --- | --- |
| Drift before commit | 5 | `CONCURRENT_MODIFICATION` |
| Root locked | 5 | `ROOT_LOCKED` |
| Incomplete journal at start | 6 | `RECOVERY_REQUIRED` |
| Commit I/O (after rollback attempt) | 3 | `ATOMIC_COMMIT_FAILED` |
| Incomplete rollback | 6 | `ROLLBACK_FAILED` (list paths; keep journal) |
| Ambiguous crash state | 6 | `RECOVERY_AMBIGUOUS` |

## Upstream contrast

| System | Behavior |
| --- | --- |
| Codex | Sequential FS apply; `AppliedPatchDelta`; `015_*leaves_changes` keeps earlier ops |
| Agents `WorkspaceEditor` | Per-op write immediately |
| `agent-patch` | No visible write until durable PREPARED journal; recover to all-before or all-after |

## Fault injection

Killpoints (`AGENT_PATCH_FAILPOINT` under `--features failpoints`): `after_prepared`, `before_visible_mutate`, `after_first_visible`, `before_completed`. Coverage: `cargo test --features failpoints --test crash_matrix`. Incomplete journals block new writers until `recover`. Dead-PID locks may be reclaimed without deleting journals.
