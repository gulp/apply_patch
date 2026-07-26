# Threat model

Assumes patch input may be malformed or adversarial.

## Threats

- Path traversal and symlink escape outside `--root`
- Overwriting arbitrary host files
- Special-file writes (device, FIFO, socket, directory)
- Resource exhaustion (huge patch / file / hunk counts)
- Ambiguous context causing unintended edits
- TOCTOU modification between validate and commit
- Temporary-file races
- Accidental secret exposure in logs

## Controls

1. Canonicalize the root once; reject absolute paths, `..`, and NUL.
2. Resolve and inspect ancestors; reject symlink escape outside root.
3. Reject non-regular targets for update/delete; refuse symlink targets.
4. Exclusive same-directory temp files with unpredictable names; atomic rename.
5. Enforce patch, file, and hunk limits before heavy work.
6. Unique exact matching only (no fuzzy default, no first-match-wins); `*** End of File` still exact (EOF-prefer, then unique forward).
7. Fingerprint revalidation immediately before visible mutation.
8. No shell-out from the apply runtime; no interpreting file content as commands.
9. Keep diagnostics content-bounded; default logs omit source bodies.
10. `--check` performs full validation without writes.

Design detail: [design/transaction.md](./design/transaction.md), [design/overview.md](./design/overview.md).
