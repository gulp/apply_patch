# Transaction journal and recovery

Companion to [contract-v2.md](../contract-v2.md). Layout under `<root>/.agent-patch/`.

Ground truth contrast: Codex records committed mutations in `AppliedPatchDelta` (`exact: bool`) and leaves earlier ops on failure (`015_failure_after_partial_success_leaves_changes`). Journals + CAS before-images reject that shape. See [research-post-v1-seams.md](../research-post-v1-seams.md).

```rust
// openai/codex …/apply-patch/src/lib.rs — accounting, not crash recovery
pub struct AppliedPatchDelta {
    changes: Vec<AppliedPatchChange>,
    exact: bool,
}
```

## Layout

```text
.agent-patch/
├── lock
├── objects/<blake3>          # immutable before-images
├── transactions/<txid>/
│   └── journal.json
├── receipts/<txid>.json
├── shadows/<id>/             # verify workspaces
└── events/                   # optional JSONL (AGENT_PATCH_EVENT_LOG)
```

## Journal states

| State | Meaning |
| --- | --- |
| `PREPARED` | Before-images + temps durable; no visible rename/delete yet |
| `COMMITTING` | At least one visible mutation started; progress per entry |
| `COMPLETED` | After-identities verified; receipt durable |
| `ROLLING_BACK` | Restoring before-states |
| `ROLLED_BACK` | Before-identities verified |

Linearization point: first successful visible rename or delete.

## Recovery decision table

| Observed | Action |
| --- | --- |
| Journal `COMPLETED` | No-op; cleanup temps if any |
| Journal `PREPARED` (no visible mutations) | Remove temps; mark `ROLLED_BACK` or delete transaction dir |
| Journal `COMMITTING` and every after-identity already matches plan | Finalize receipt if needed; mark `COMPLETED` |
| Journal `COMMITTING` / `ROLLING_BACK` and not all after | Restore **all** before-states from objects; mark `ROLLED_BACK` |
| Missing/corrupt object or ambiguous mix | Retain journal; `RECOVERY_AMBIGUOUS` (exit 6); do not invent mixed state |

Rules:

1. Never mixed roll-forward.
2. Never delete journals via stale-lock heuristics.
3. Mutating commands refuse to start while any incomplete journal exists (`RECOVERY_REQUIRED`).
4. Lock is advisory defense in depth; content revalidation always runs.
5. A lock file whose recorded PID is not alive may be reclaimed; journals are never deleted by that path.
6. `COMMITTING` with a mix of before/after file identities restores **all** before-images (not Ambiguous), provided every path matches either before or after.

## Object GC

Mark-and-sweep over receipts + incomplete journals. Explicit `gc [--dry-run]` only. Never remove referenced objects.
