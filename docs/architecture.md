# Architecture

See [design/overview.md](./design/overview.md).

```text
CLI → app → parse → path policy → snapshot → validate → plan
         → engine (locate_chunks → emit_chunks)
         → [--check|--plan] emit and stop
         → [--verify] shadow → verify argv → promote
         → commit (lock, objects, journal, rename, receipt)
         → recover / revert / gc / status / doctor
```

Invariants: root confinement; no mutation before full in-memory plan; unique hunk matches (EOF-prefer when marked; optional unique-only fuzz); durable journal before visible mutation; transactional commit with rollback/recover; stable diagnostics.

| Topic | Doc |
| --- | --- |
| Contract (baseline) | [contract-v1.md](./contract-v1.md) |
| Contract (plans/verify/journals/receipts) | [contract-v2.md](./contract-v2.md) |
| Schemas | [schemas/](./schemas/) |
| Journal / recovery | [design/transaction-journal.md](./design/transaction-journal.md) |
| Protocol | [protocol.md](./protocol.md) |
| Errors | [errors.md](./errors.md) |
| Design | [design/](./design/) |
| Threat model | [threat-model.md](./threat-model.md) |
| Codex fixture subset | `crates/agent-patch/tests/fixtures/codex-scenarios/` |
| Active plan | [../IMPLEMENTATION_PLAN.md](../IMPLEMENTATION_PLAN.md) |
| Backlog | [research-next-pass.md](./research-next-pass.md) |
| Seam ground truth | [research-post-v1-seams.md](./research-post-v1-seams.md) |
| Archived greenfield plan | [archive/2026-07-greenfield-implementation-plan.md](./archive/2026-07-greenfield-implementation-plan.md) |
