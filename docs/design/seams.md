# Seams

## External

```text
agents/CI → CLI → app::run
                    ├─ parse_patch
                    ├─ path + snapshot
                    ├─ apply_update
                    └─ commit_plan(fs)
```

| Seam | Depth | Tests |
| --- | --- | --- |
| `parse_patch` | Grammar + spans (incl. `@@` anchors, `*** End of File`) | Malformed corpus; no tempdir |
| `apply_update` | `locate_chunks` + `emit_chunks` + newlines | String fixtures; CRLF matrix; EOF |
| `commit_plan` | Revalidate + temps + rollback | Tempdirs + fault `FileSystem` |
| `app::run` | Wire + public errors/exits | `assert_cmd`; Codex fixtures; `scripts/dogfood` |

## Adapters

| Seam | Prod | Test |
| --- | --- | --- |
| `FileSystem` | `RealFileSystem` | `CountingFs`, fault injector |

No `diffy`/`flickzeug` apply adapter — wrong dialect (unified diff). `similar` stays a private helper in `diff_summary.rs` until a second summary backend appears.

Optional later: `HunkLocator` (exact unique vs explicit fuzzy unique) — only when `--fuzzy` exists.

## Post-v1 planned seams

| Seam | Prod intent | Ground truth |
| --- | --- | --- |
| `ExecutionPlan` + digest | Freeze after locate/emit | Agents `Chunk` / `_apply_chunks`; no upstream digest |
| `--fuzzy` unique ladder | Opt-in rstrip/strip | Codex `seek_sequence` levels; **unique** gate is ours |
| `--idempotent` | Full after-state proof | flickzeug `ApplyOutcome` is UX analogy only (unified) |
| Journal + objects + `recover` | Crash all-before/after | Contrast Codex `AppliedPatchDelta` (non-atomic) |
| Verify runner | argv + process group + budgets | `process_group(0)` + group kill; `kill_on_drop` patterns |

Details and code samples: [research-post-v1-seams.md](../research-post-v1-seams.md).

## Internal (private)

Engine: `locate_chunks`, `emit_chunks`.  
Commit: `revalidate`, `prepare_temps`, `commit_entries`, `rollback`.

## Layout

`crates/agent-patch/src/` — one concern per file. Engine split: `engine/{matcher,locate,emit,apply,diff_summary}.rs`.

## Avoid

- Matching inside `commit.rs` or writing files inside `apply_update`
- Public matcher knobs in v1 CLI
- Treating workspace `diffy` / `flickzeug` as the V4A engine
- Path-policy logic duplicated outside `path_policy.rs`
