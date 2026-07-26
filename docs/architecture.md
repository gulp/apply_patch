# Architecture

See [design/overview.md](./design/overview.md).

```text
CLI → app → parse → path policy → snapshot → validate → plan/engine → commit → fs
```

Invariants: root confinement; no mutation before full in-memory apply; unique hunk matches; transactional commit with rollback; stable diagnostics.

| Topic | Doc |
| --- | --- |
| Contract | [contract-v1.md](./contract-v1.md) |
| Protocol | [protocol.md](./protocol.md) |
| Errors | [errors.md](./errors.md) |
| Design | [design/](./design/) |
| Threat model | [threat-model.md](./threat-model.md) |
| Next-pass backlog | [research-next-pass.md](./research-next-pass.md) |
