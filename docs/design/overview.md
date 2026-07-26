# Elegant Solution Overview

## Problem

Coding agents need localized, multi-file edits that are:

- **model-legible** (Codex / OpenAI Agents V4A `*** Begin Patch` dialect);
- **safe** (no path escape, no silent wrong edit, no partial tree);
- **harness-simple** (one CLI, stdin or file, stable exits and JSON).

Whole-file rewrite and fuzzy “best effort” apply both fail those constraints.

## Solution shape

Three deep modules, one pipeline:

```text
┌─────────────────────────────────────────────────────────────┐
│ CLI (clap)                                                  │
│   stdin|file → AppConfig → stdout/stderr/exit               │
└───────────────────────────┬─────────────────────────────────┘
                            ▼
┌─────────────────────────────────────────────────────────────┐
│ Application service                                         │
│   orchestrate phases; no matching; no FS writes             │
└───────────────────────────┬─────────────────────────────────┘
          ┌─────────────────┼─────────────────┐
          ▼                 ▼                 ▼
   ┌────────────┐   ┌──────────────┐   ┌─────────────┐
   │ Protocol   │   │ Path+Snapshot│   │ Apply engine│
   │ parse only │   │ policy+load  │   │ pure text   │
   └────────────┘   └──────────────┘   └─────────────┘
                            │
                            ▼
                   ┌─────────────────┐
                   │ Commit          │
                   │ revalidate+FS   │
                   └─────────────────┘
```

| Module | Knows | Must not know |
| --- | --- | --- |
| Protocol | Envelope grammar, spans | Filesystem, matching fuzz |
| Path/Snapshot | Root confinement, bytes, fingerprints | Hunk syntax |
| Apply engine | Locate + transform text | Paths, writes, JSON |
| Commit | Temp files, rename, rollback | Patch dialect |
| CLI | Args, I/O streams | Patch semantics |

## Inheritance map

Aligned with OpenAI stacks:

1. **V4A envelope** — `Begin/End Patch`, Add / Update / Delete (Move deferred).
2. **Headerless per-file diff body** for updates — `@@` + ` `/`-`/`+` lines.
3. **Locate-then-emit chunks** — resolve match positions on the original line array, then apply with a forward cursor.
4. **`@@` as a seek anchor** — narrows the search window; does not authorize fuzzy line equality.
5. **`similar` for observation only** — line counts / optional unified summary after success.
6. **Newline policy** — normalize to LF for matching; re-emit using the file’s LF/CRLF.
7. **Portable scenario fixtures** — `input/` + `patch.txt` + `expected/`.

Deliberately different:

1. **No `diffy` as apply backend** — `diffy` is for unified-diff display elsewhere; apply is custom.
2. **Unique match required** — zero hits → `HUNK_NOT_FOUND`; multiple → `HUNK_AMBIGUOUS`.
3. **No silent whitespace / Unicode fuzz** in default mode.
4. **Transactional commit** — validation failure or mid-commit failure leaves no partial intended tree (rollback).
5. **Add does not overwrite** — existing path → `FILE_ALREADY_EXISTS`.

## Differentiator

```text
Typical agent apply paths:  success rate  ↑   via fuzzy locate + sequential writes
agent-patch:                safety        ↑   via unique exact locate + transactional commit
```

Same dialect so agents can transfer skill; fail-closed runtime so harnesses can trust the tree.

## End-to-end data flow

```text
patch bytes
  → limit check
  → parse envelope → PatchDocument
  → validate paths + limits + duplicates
  → canonicalize root; resolve under root (no symlink escape)
  → snapshot each path (Missing | Present{bytes, text, blake3, newline, perms})
  → validate op↔state (Add/Update/Delete rules; reject mixed newlines on update)
  → for each Update: apply_engine(text, hunks) → final bytes   ⎫ all in memory
    for each Add:    materialize create bytes                   ⎬ no FS mutation
    for each Delete: plan remove                                ⎭
  → reject PATCH_NO_EFFECT if nothing changes
  → if --check: emit success and stop
  → revalidate fingerprints
  → prepare temps → commit → rollback on failure
  → emit human or JSON summary
```

## Public surface

```text
scripts/agent-patch [--check] [--json] [--quiet] [--root PATH]
                    [--max-files N] [--max-patch-bytes N] [--max-file-bytes N]
                    [PATCH_FILE]
```

Exit codes and error codes: [`../errors.md`](../errors.md), [`../contract-v1.md`](../contract-v1.md).

## Non-goals

No AST transforms, no fuzzy default, no Git staging, no MCP requirement, no binary files, no interactive resolution, no whole-file rewrite fallback.

## Roadmap sketch

| Horizon | Scope |
| --- | --- |
| Engine hardening | Locate-all → cursor emit refactor; CRLF matrix tests |
| v1.1 | Optional `*** Move to:`; optional `--fuzzy` with unique match at chosen fuzz level; `*** End of File` |
| Corpus | Scenario fixtures aligned with Codex where semantics match |

## Related docs

- Contract: [`../contract-v1.md`](../contract-v1.md)
- Protocol: [`../protocol.md`](../protocol.md)
- Research: [`../research-codex-apply-patch.md`](../research-codex-apply-patch.md), [`../research-openai-agents-apply-diff.md`](../research-openai-agents-apply-diff.md)
