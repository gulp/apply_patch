# Threat model

Assumes patch input may be malformed or adversarial. Verify commands run with the invoking user’s privileges (not a sandbox).

## Threats

- Path traversal and symlink escape outside `--root`
- Overwriting arbitrary host files
- Special-file writes (device, FIFO, socket, directory)
- Resource exhaustion (huge patch / file / hunk counts; shadow / verify budgets)
- Ambiguous context causing unintended edits
- TOCTOU modification between validate and commit
- Temporary-file races and hard-link aliasing into the real tree
- Crash leaving a mixed before/after tree
- Accidental secret exposure in logs or verify artifacts
- Untrusted `--verify` argv (user-chosen; same privileges as the agent)

## Controls

1. Canonicalize the root once; reject absolute paths, `..`, and NUL.
2. Resolve and inspect ancestors; reject symlink escape outside root.
3. Reject non-regular targets for update/delete; refuse symlink targets.
4. Exclusive same-directory temp files with unpredictable names; atomic rename.
5. Enforce patch, file, hunk, shadow, and verify limits before heavy work / before verifier launch.
6. Unique matching only (no fuzzy default, no first-match-wins); optional `--fuzzy` still unique-only; `*** End of File` still exact (EOF-prefer, then unique forward).
7. Fingerprint revalidation immediately before visible mutation; optional hash pins before locate.
8. Durable journal + CAS before-images before first visible rename/delete; incomplete journals block new writers until `recover`.
9. Root lock for mutate / revert / recover / gc; never delete journals via stale-lock heuristics.
10. Hard links forbidden in shadows and transactional storage.
11. No shell-out from the default apply path; `--verify` runs explicit argv only.
12. Keep diagnostics and verify streams content-bounded; default logs omit source bodies.
13. `--check` / `--plan` perform full validation without writes; verify failure discards the shadow and leaves the root unchanged.
14. Receipts are self-contained (object-backed); hashes-only receipts are rejected.

Design detail: [design/transaction.md](./design/transaction.md), [design/transaction-journal.md](./design/transaction-journal.md), [design/overview.md](./design/overview.md), [contract-v2.md](./contract-v2.md).
