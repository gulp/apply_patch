# Architecture

Canonical design: [`design/overview.md`](./design/overview.md).

```text
CLI → app → parse → path policy → snapshot → validate → plan/engine → commit → fs
```

Invariants: root confinement, no mutation before full in-memory apply, unique hunk matches, transactional commit with rollback, stable diagnostics.

| Topic | Doc |
| --- | --- |
| Contract | [contract-v1.md](./contract-v1.md) |
| Protocol | [protocol.md](./protocol.md) |
| Errors | [errors.md](./errors.md) |
| Apply engine | [design/apply-engine.md](./design/apply-engine.md) |
| Transactions | [design/transaction.md](./design/transaction.md) |
| Seams | [design/seams.md](./design/seams.md) |
| Stack | [design/stack.txt](./design/stack.txt) |
| Threat model | [threat-model.md](./threat-model.md) |
