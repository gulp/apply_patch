# JSON Schemas (contract v2)

Frozen shapes for plan / receipt / journal artifacts. Public CLI JSON uses `version: 2` when plan, transaction, oracle, or verify fields are present. See [contract-v2.md](../contract-v2.md).

| Schema | Artifact |
| --- | --- |
| [execution-plan.schema.json](./execution-plan.schema.json) | `--plan` / immutable `ExecutionPlan` |
| [receipt.schema.json](./receipt.schema.json) | Apply / revert receipts under `.agent-patch/receipts/` |
| [journal.schema.json](./journal.schema.json) | `.agent-patch/transactions/*/journal.json` |

Canonical plan digest encoding: sorted repo-relative paths; no unordered maps in digest-bearing structures; `plan_digest` is `blake3:` + 64 hex chars.

Receipt `permissions` may include `executable` and optional Unix `mode` bits used by `revert`.
