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
