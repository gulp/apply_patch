# Architecture

See [design/overview.md](./design/overview.md).

```text
CLI → app → parse → path policy → snapshot → validate → plan
         → engine (locate_chunks → emit_chunks) → commit → fs
```

Invariants: root confinement; no mutation before full in-memory apply; unique hunk matches (EOF-prefer when marked); transactional commit with rollback; stable diagnostics.

| Topic | Doc |
| --- | --- |
| Contract | [contract-v1.md](./contract-v1.md) |
| Protocol | [protocol.md](./protocol.md) |
| Errors | [errors.md](./errors.md) |
| Design | [design/](./design/) |
| Threat model | [threat-model.md](./threat-model.md) |
| Codex fixture subset | `crates/agent-patch/tests/fixtures/codex-scenarios/` |
| Active plan | [../IMPLEMENTATION_PLAN.md](../IMPLEMENTATION_PLAN.md) |
| Backlog | [research-next-pass.md](./research-next-pass.md) |
| Archived greenfield plan | [archive/2026-07-greenfield-implementation-plan.md](./archive/2026-07-greenfield-implementation-plan.md) |
