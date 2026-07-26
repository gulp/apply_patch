# Seams and Deep Modules

Vocabulary: **module**, **interface**, **seam**, **adapter**, **depth** (see codebase-design skill).

## External seams (stable)

```text
                    ┌──────────────────┐
   agents / CI ───► │ CLI binary       │  small: flags + exit + JSON|human
                    └────────┬─────────┘
                             │ AppConfig → AppOutput
                    ┌────────▼─────────┐
                    │ app::run         │  orchestration only
                    └────────┬─────────┘
           ┌─────────────────┼──────────────────┐
           ▼                 ▼                  ▼
    parse_patch        path + snapshot      apply_update
    (protocol)         (policy)             (engine)
           │                 │                  │
           └────────────┬────┴──────────────────┘
                        ▼
                   commit_plan (fs adapter)
```

### 1. `parse_patch(text) -> PatchDocument`

- **Depth:** full grammar, spans, lenient-vs-strict choices hidden.
- **Tests:** malformed corpus; never needs a tempdir.

### 2. `apply_update(base, hunks, newline, …) -> AppliedText`

- **Depth:** locate + emit + newline/BOM rules.
- **Tests:** string fixtures only (Agents examples, ambiguity, CRLF).

### 3. `commit_plan(fs, plan, limits) -> CommitResult`

- **Depth:** revalidate, temps, rename, rollback.
- **Tests:** real tempdirs + fault-injecting `FileSystem` adapter.

### 4. `app::run(config) -> AppOutput`

- **Depth:** wires the above; maps errors to public codes/exits.
- **Tests:** CLI integration (`assert_cmd`); dogfood script.

## Real seams (two adapters)

| Seam | Production adapter | Test adapter |
| --- | --- | --- |
| `FileSystem` | `RealFileSystem` | `CountingFs`, fault injector |
| (future) `HunkLocator` | exact unique | optional fuzzy unique |

Do **not** introduce a `diffy` adapter seam — research showed it is the wrong dialect. Observation stays on `similar` behind a tiny `diff_summary` helper (one adapter is enough; no trait required until a second summary backend appears).

## Internal seams (private)

Inside the apply engine:

- `locate_chunks(lines, hunks) -> Vec<Chunk>`
- `emit_chunks(lines, chunks, newline) -> String`

Inside commit:

- `revalidate`
- `prepare_temps`
- `commit_entries`
- `rollback`

Callers outside the module must not see these.

## Deletion test

| If we deleted… | Complexity reappears in… |
| --- | --- |
| Protocol parser | every caller invents Begin/End parsing |
| Apply engine | CLI and commit would reimplement matching |
| Commit coordinator | partial writes and races leak into app |
| Path policy | symlink escape bugs scatter |

Each pays rent.

## AI-navigable layout

Keep one concern per file under `crates/agent-patch/src/` as in the implementation plan. Prefer growing `engine/{locate,emit}.rs` over stuffing CLI with match logic.

## Anti-patterns to avoid

- Passing `diffy::Patch` through the apply path “because Codex depends on diffy” (it doesn’t, for apply).
- Matching inside `commit.rs`.
- Writing files inside `apply_update`.
- Expanding CLI flags for matcher algorithm knobs in v1 (contract forbids).
