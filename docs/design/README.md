# Design

`agent-patch` architecture: V4A envelope, pure-text unique-exact apply, transactional commit.

| Doc | Purpose |
| --- | --- |
| [overview.md](./overview.md) | Pipeline, ground-truth map, non-goals |
| [apply-engine.md](./apply-engine.md) | Locate → emit; matching and newlines |
| [transaction.md](./transaction.md) | Snapshot, commit, rollback |
| [seams.md](./seams.md) | Module boundaries and tests |
| [stack.txt](./stack.txt) | Crates in / out of the apply path |

**Thesis:** Parse the V4A envelope once; transform each file as pure text with unique exact matching; commit all-or-nothing. Never apply via `diffy`/`flickzeug`; never silent fuzzy fallback.

Contract: [`../contract-v1.md`](../contract-v1.md). Upstream notes: [`../research-codex-apply-patch.md`](../research-codex-apply-patch.md), [`../research-openai-agents-apply-diff.md`](../research-openai-agents-apply-diff.md), [`../research-next-pass.md`](../research-next-pass.md).
