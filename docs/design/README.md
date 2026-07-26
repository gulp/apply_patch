# Design

Architecture for `agent-patch`: Codex/V4A envelope, pure-text apply with unique exact matching, transactional commit.

Related stacks for dialect and apply-shape reference: OpenAI Codex `codex-apply-patch`, OpenAI Agents `apply_diff` / `applyDiff`, Zed `codex-acp` (display vs apply). Product contract: [`../contract-v1.md`](../contract-v1.md).

## Documents

| Doc | Purpose |
| --- | --- |
| [overview.md](./overview.md) | Thesis, layers, inheritance map |
| [apply-engine.md](./apply-engine.md) | Pure-text apply: locate → chunks → cursor emit |
| [transaction.md](./transaction.md) | Snapshot, plan, commit, rollback |
| [seams.md](./seams.md) | Deep modules and test surfaces |
| [stack.txt](./stack.txt) | Chosen crates |

## Thesis

**Parse the Codex/V4A envelope once, apply each file as a pure string transform with unique exact matching, then commit all results transactionally — never through `diffy`, never with silent fuzzy fallback.**
