# `agent-patch` — Implementation Plan

Status: **Stub** — next planning topic (not a full §1–21 plan yet)
Authoritative behavior today: [README.md](README.md), [docs/contract-v1.md](docs/contract-v1.md), [docs/contract-v2.md](docs/contract-v2.md), [docs/protocol.md](docs/protocol.md), [docs/design/](docs/design/)

## Completed plans (archived)

| Plan | Role |
| --- | --- |
| [docs/archive/2026-07-greenfield-implementation-plan.md](docs/archive/2026-07-greenfield-implementation-plan.md) | v1 greenfield CLI |
| [docs/archive/2026-07-27-post-v1-implementation-plan.md](docs/archive/2026-07-27-post-v1-implementation-plan.md) | Post-v1 reliability (oracle, verify, receipts, fuzzy/risk, doctor) |

Intentional backlog (unchanged): `Move File` / `translate`.

## Next topic — agent / robot CLI UX

Goals (one-liners):

1. Explicit machine mode (`--robot` / `AGENT_PATCH_ROBOT` / existing `--json`) with pure JSON stdout.
2. Small unique argv rewrite allowlist for known agent footguns, with `coach` on success.
3. Fail closed with ≥2 copy-paste `examples` when intent is ambiguous; suggest-only for unknown flags.
4. In-tool `robot-docs`; keep AGENTS/skill/help in sync on flag changes.

Design freezes and catalog: [docs/design/robot-cli.md](docs/design/robot-cli.md).
