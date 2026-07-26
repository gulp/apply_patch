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
| `parse_patch` | Grammar + spans | Malformed corpus; no tempdir |
| `apply_update` | Locate + emit + newlines | String fixtures |
| `commit_plan` | Revalidate + temps + rollback | Tempdirs + fault `FileSystem` |
| `app::run` | Wire + public errors/exits | `assert_cmd`; `scripts/dogfood` |

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

`crates/agent-patch/src/` — one concern per file. Prefer `engine/{locate,emit}.rs` over growing CLI/match logic in `app.rs`.

## Avoid

- Matching inside `commit.rs` or writing files inside `apply_update`
- Public matcher knobs in v1 CLI
- Treating workspace `diffy` / `flickzeug` as the V4A engine
- Path-policy logic duplicated outside `path_policy.rs`
