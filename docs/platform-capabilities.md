# Platform capabilities (Linux / macOS)

Present-tense notes for operators and agents. Source of truth for behavior remains `docs/contract-v2.md` and the crate under `crates/agent-patch`.

## Supported platforms

| Platform | CI | Notes |
| --- | --- | --- |
| Linux | `ubuntu-latest` | Primary development target |
| macOS | `macos-latest` | Same public CLI and journal semantics |

Windows is out of scope for this release.

## Filesystem

| Capability | Linux | macOS | Behavior |
| --- | --- | --- | --- |
| Same-directory atomic rename | required | required | Commit temps promote via rename; unsupported layouts fail closed |
| `fsync` / durable journal | used | used | Before-images and `PREPARED` journals are durable before visible mutation |
| Reflink / clonefile acceleration | best-effort via `std::fs::copy` | best-effort (clonefile when available through copy) | Never hard-link shadows or CAS objects |
| Symlinks in shadows | copied as symlinks only when lexical + resolved targets stay in root | same | Escape → verify fails by policy |

Multi-file **atomic visibility** is not claimed. Guarantee: recoverability to proven all-before or all-after.

## Process / verify runner

| Capability | Linux | macOS |
| --- | --- | --- |
| New process group for verify children | `setpgid(0,0)` | same |
| Timeout kill | `SIGTERM` then `SIGKILL` to the process group | same |
| `--verify-shell` | `/bin/sh -c` | `/bin/sh -c` |

Gates: `cargo test --test verify_process_reaping`; soak: `scripts/soak`.

## Locking / recover

| Capability | Linux | macOS |
| --- | --- | --- |
| Root advisory lock | `.agent-patch/lock` with PID | same |
| Stale lock reclaim | holder PID not alive (`kill(pid, 0)`) | same |
| Journal deletion via lock heuristics | **never** | **never** |

Killpoint coverage: `cargo test --features failpoints --test crash_matrix`.

## Observability

Optional JSONL via `AGENT_PATCH_EVENT_LOG=1` or a file path only (no `--event-log` flag). Failures never change exit codes.
