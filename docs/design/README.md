# Design

`agent-patch` architecture: V4A envelope, pure-text unique matching, journaled transactional commit, verify shadows, self-contained receipts.

| Doc | Purpose |
| --- | --- |
| [overview.md](./overview.md) | Pipeline, ground-truth map, non-goals |
| [apply-engine.md](./apply-engine.md) | Locate → emit; matching and newlines |
| [transaction.md](./transaction.md) | Snapshot, commit, rollback |
| [transaction-journal.md](./transaction-journal.md) | Durable journal, recovery table, `.agent-patch/` layout |
| [move.md](./move.md) | Deferred Move File commit-order rules |
| [seams.md](./seams.md) | Module boundaries and tests |
| [stack.txt](./stack.txt) | Crates in / out of the apply path |

**Thesis:** Parse the V4A envelope once; transform each file as pure text with unique matching (`locate_chunks` → `emit_chunks`); commit through a durable journal and content-addressed before-images. Never apply via `diffy`/`flickzeug`; never silent first-match fuzzy fallback; never hard-link shadows.

Contract: [`../contract-v1.md`](../contract-v1.md) (baseline matching/ops), [`../contract-v2.md`](../contract-v2.md) (plans, verify / verify-shell, journals, receipts, fuzzy/risk/idempotent, event log), [`../schemas/`](../schemas/). Upstream notes: [`../research-codex-apply-patch.md`](../research-codex-apply-patch.md), [`../research-openai-agents-apply-diff.md`](../research-openai-agents-apply-diff.md), [`../research-post-v1-seams.md`](../research-post-v1-seams.md), [`../research-next-pass.md`](../research-next-pass.md).
