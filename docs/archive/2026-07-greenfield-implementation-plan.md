# `agent-patch` — Implementation Plan

Status: **Archived** greenfield v1 plan (completed). Current behavior: [README.md](../../README.md), [docs/contract-v1.md](../contract-v1.md), [docs/protocol.md](../protocol.md), [docs/design/](../design/). Post-v1 plan (also archived): [2026-07-27-post-v1-implementation-plan.md](./2026-07-27-post-v1-implementation-plan.md). Root stub: [IMPLEMENTATION_PLAN.md](../../IMPLEMENTATION_PLAN.md).
Primary users: Coding agents operating through shell-capable harnesses
Primary interface: Repo-local command-line executable
Implementation language: Rust
Initial platforms: Linux and macOS
Primary objective: Apply localized code changes safely without rewriting entire files

---

## 1. Goals / Non-goals

### 1.1 Goals

1. Provide a deterministic, non-interactive CLI that coding agents can use to apply localized edits to repository files.

2. Support a model-legible patch protocol with explicit operations:
   - add file;
   - update file;
   - delete file;
   - move or rename file, if included in the initial contract;
   - apply multiple file operations atomically.

3. Preserve all unaffected file content exactly.

4. Reject malformed, ambiguous, stale, unsafe, or partially applicable patches before mutating the working tree.

5. Make every failure actionable through:
   - stable exit codes;
   - machine-readable error codes;
   - concise human-readable diagnostics;
   - explicit next-action hints.

6. Ensure a patch either:
   - applies completely; or
   - makes no filesystem changes.

7. Allow coding agents to invoke the tool through ordinary shell execution without MCP registration or harness-specific tool integration.

8. Support both:
   - patch text supplied over standard input;
   - patch text supplied through a file path.

9. Provide a validation-only mode that performs all parsing, safety checks, and in-memory application without writing files.

10. Detect concurrent modification between validation and commit.

11. Preserve:

- line-ending style;
- final-newline state;
- file permissions where supported;
- unaffected bytes;
- path casing and repository-relative path identity.

12. Produce machine-pure output suitable for coding-agent harnesses.

13. Keep the protocol and observable behavior stable enough that multiple coding agents can generate patches independently against the same contract.

14. Provide repository-local installation and invocation:

```bash
scripts/agent-patch
```

15. Make incorrect operation types impossible or explicit:

- `Add File` must target a missing path;
- `Update File` must target an existing regular file;
- `Delete File` must target an existing file;
- unsafe paths must be rejected.

### 1.2 Non-goals

1. `agent-patch` is not a general-purpose text editor.

2. It will not perform semantic code transformations using ASTs or language servers.

3. It will not infer the user’s intended change.

4. It will not generate patches from natural-language instructions.

5. It will not silently fall back to:
   - full-file rewrites;
   - fuzzy nearest-match selection;
   - whitespace-insensitive matching;
   - line-number-only matching;
   - external `patch` or `git apply`;
   - three-way merge without explicit base data.

6. It will not automatically resolve semantic merge conflicts.

7. It will not invoke formatters, compilers, linters, or tests.

8. It will not stage files in Git.

9. It will not create commits, branches, or pull requests.

10. It will not modify files outside the configured root.

11. It will not follow symbolic links outside the configured root.

12. It will not modify binary files in v1.

13. It will not support arbitrary encodings in v1. Initial support is UTF-8 text with optional UTF-8 BOM handling.

14. It will not support interactive conflict resolution.

15. It will not expose backend-specific concepts such as `diffy` fuzz parameters or `similar` algorithm selection through the public CLI in v1.

16. It will not promise compatibility with arbitrary unified-diff dialects unless explicitly included in the protocol contract.

---

## 2. Product Definition

### 2.1 Project

`agent-patch`

A repo-local Rust CLI that allows coding agents to apply structured, localized, transactional file changes using a deterministic patch protocol.

### 2.2 Users

Primary users:

- Claude Code agents;
- Codex-style coding agents;
- shell-capable autonomous development agents;
- human developers reviewing or replaying agent-generated patches;
- CI systems validating generated patches.

Secondary users:

- agent harness authors;
- repository maintainers;
- multi-agent orchestration systems;
- code-review automation.

### 2.3 Problem statement

Coding agents frequently rewrite entire files to make localized changes. Whole-file replacement creates unnecessary risk:

- accidental removal of unrelated edits;
- formatting churn;
- stale-content overwrite;
- loss of comments or whitespace;
- merge conflicts;
- poor reviewability;
- excessive output tokens;
- inability to distinguish intended from incidental changes;
- difficult recovery when concurrent edits occur.

Existing exact string-replacement tools work well for simple, unique replacements but become awkward for:

- multi-hunk edits;
- additions and removals around stable context;
- coherent changes across several files;
- user-supplied patches;
- transactional application;
- stale line numbers with otherwise valid surrounding context.

`agent-patch` provides a strict localized-edit runtime with explicit validation, contextual matching, atomic commit, and stable diagnostics.

### 2.4 Constraints

The implementation must be:

- deterministic;
- non-interactive;
- offline;
- repo-local;
- safe by default;
- cross-platform across Linux and macOS;
- suitable for invocation through `Bash`;
- independent of Claude Code, Codex, MCP, or any single harness;
- machine-readable;
- idempotence-aware;
- atomic across all paths in one patch;
- resistant to path traversal and symlink escape;
- robust under concurrent file modification;
- bounded in memory and execution time;
- explicit about unsupported input;
- free of silent fallback behavior.

### 2.5 Environment

Initial stack:

- Rust stable;
- Cargo workspace;
- `clap` for CLI parsing;
- `serde` and `serde_json` for structured output;
- `similar` for resulting-diff computation and diagnostics;
- `diffy` or a maintained equivalent such as `flickzeug` for contextual hunk application where its semantics match the project contract;
- `thiserror` for typed internal errors;
- `tempfile` for safe temporary files;
- `sha2` or `blake3` for content fingerprints;
- `camino` or standard `PathBuf` with strict normalization helpers;
- `fs2` or platform-specific locking only if required by the finalized concurrency design;
- `proptest` for parser and path-safety property tests;
- `assert_cmd` and `predicates` for CLI integration tests;
- `insta` optionally for stable diagnostic snapshots;
- GitHub Actions for Linux and macOS validation.

No runtime service, database, network access, daemon, or configuration server is required.

---

## 3. User-Facing Contract

### 3.1 Canonical invocation

Read a patch from standard input:

```bash
scripts/agent-patch <<'PATCH'
*** Begin Patch
*** Update File: src/config.rs
@@
 pub const RETRIES: usize = 2;
-pub const TIMEOUT_SECS: u64 = 30;
+pub const TIMEOUT_SECS: u64 = 45;
*** End Patch
PATCH
```

Validate without writing:

```bash
scripts/agent-patch --check < /tmp/change.patch
```

Read from a patch file:

```bash
scripts/agent-patch /tmp/change.patch
```

Emit JSON:

```bash
scripts/agent-patch --json < /tmp/change.patch
```

Constrain the root explicitly:

```bash
scripts/agent-patch --root "$PWD" < /tmp/change.patch
```

### 3.2 Initial command surface

```text
agent-patch [OPTIONS] [PATCH_FILE]
```

Options:

```text
--check
--root <PATH>
--json
--quiet
--max-files <N>
--max-patch-bytes <N>
--max-file-bytes <N>
--version
--help
```

Default behavior:

- read patch from `PATCH_FILE` when provided;
- otherwise read patch from standard input;
- resolve root from `--root`, otherwise current working directory;
- parse entire patch;
- validate every operation;
- load every affected file;
- apply every operation in memory;
- revalidate current file fingerprints;
- commit all writes;
- print concise success output;
- return exit code `0`.

### 3.3 Protocol

Canonical envelope:

```text
*** Begin Patch
<one or more file operations>
*** End Patch
```

Supported operations:

```text
*** Add File: path/to/file
+line one
+line two
```

```text
*** Update File: path/to/file
@@
 context
-old
+new
 context
```

```text
*** Delete File: path/to/file
```

Optional rename support, only if implemented as part of v1:

```text
*** Move File: old/path
*** To: new/path
```

Recommendation: defer move support until v1.1 unless required by a concrete agent workflow. Move semantics complicate path-collision, permissions, rollback, and cross-device guarantees.

### 3.4 Patch grammar constraints

1. Exactly one `*** Begin Patch` line.
2. Exactly one `*** End Patch` line.
3. No non-whitespace content outside the envelope.
4. Every operation begins with a recognized operation header.
5. Paths are repository-relative UTF-8 paths.
6. Absolute paths are forbidden.
7. `.` and `..` components are forbidden after normalization.
8. Empty paths are forbidden.
9. NUL bytes are forbidden.
10. Duplicate operations on the same path are forbidden in v1.
11. A patch must contain at least one operation.
12. Update hunks must contain at least one addition or deletion.
13. Empty no-op hunks are forbidden.
14. Patch size and file-count limits are enforced before loading target files.
15. Unknown headers or directives are hard errors.

### 3.5 Hunk matching contract

The matching algorithm must be deterministic and documented.

Required matching order:

1. Exact full hunk-context match.
2. Exact match after ignoring only the hunk’s advisory location metadata, if any.
3. Optional controlled edge-context reduction if this is part of the chosen protocol:
   - remove one leading context line;
   - then one trailing context line;
   - stop at the configured minimum;
   - accept only a unique remaining match.

4. If zero matches remain, fail with `HUNK_NOT_FOUND`.
5. If more than one match remains, fail with `HUNK_AMBIGUOUS`.

Forbidden matching behavior:

- edit distance ranking;
- semantic similarity;
- whitespace normalization;
- tab/space equivalence;
- case-insensitive matching;
- nearest line-number selection;
- first-match-wins under ambiguity.

Any whitespace-tolerant mode must be a future explicit flag and must never become the default.

### 3.6 Standard output and standard error

Default human mode:

- stdout: concise success summary only;
- stderr: diagnostics, warnings, and errors.

JSON mode:

- stdout: exactly one JSON object;
- stderr: reserved for process-level failures that prevent structured output, ideally empty.

No ANSI color in JSON mode.

No progress spinners.

No prompts.

No interactive confirmation.

### 3.7 Exit taxonomy

```text
0  success
1  patch does not apply to current content
2  malformed or unsupported patch
3  filesystem or I/O failure
4  unsafe path or policy violation
5  concurrent modification detected
6  internal invariant violation
7  configured resource limit exceeded
```

Exit codes are stable public API.

Internal error codes are more granular than exit codes.

Examples:

```text
PATCH_MISSING_BEGIN
PATCH_MISSING_END
PATCH_EMPTY
UNKNOWN_OPERATION
INVALID_PATH
PATH_OUTSIDE_ROOT
SYMLINK_ESCAPE
FILE_ALREADY_EXISTS
FILE_NOT_FOUND
NOT_REGULAR_FILE
BINARY_FILE_UNSUPPORTED
INVALID_UTF8
HUNK_NOT_FOUND
HUNK_AMBIGUOUS
HUNK_OVERLAP
PATCH_NO_EFFECT
CONCURRENT_MODIFICATION
ATOMIC_COMMIT_FAILED
ROLLBACK_FAILED
LIMIT_PATCH_BYTES
LIMIT_FILE_BYTES
LIMIT_FILE_COUNT
```

---

## 4. Architecture

### 4.1 Layered architecture

```text
CLI adapter
    ↓
Application service
    ↓
Protocol parser
    ↓
Validation and policy layer
    ↓
Filesystem snapshot loader
    ↓
Patch planner
    ↓
In-memory patch engine
    ↓
Commit coordinator
    ↓
Filesystem adapter
```

Supporting cross-cutting components:

```text
Diagnostics
Structured output
Hashing
Instrumentation
Limits
Path safety
Testing fixtures
```

### 4.2 Components

#### 4.2.1 CLI adapter

Responsibilities:

- parse arguments;
- determine input source;
- resolve root;
- construct application configuration;
- invoke application service;
- map typed result to:
  - stdout;
  - stderr;
  - exit code.

Must not:

- parse the patch protocol;
- mutate files;
- contain patch logic;
- guess recovery behavior.

Suggested module:

```text
src/cli.rs
```

#### 4.2.2 Input reader

Responsibilities:

- read patch bytes from stdin or file;
- enforce maximum patch size during streaming;
- reject simultaneous unsupported input combinations;
- preserve input bytes for diagnostics where safe.

Suggested module:

```text
src/input.rs
```

#### 4.2.3 Protocol parser

Responsibilities:

- tokenize and parse the patch envelope;
- parse file operations;
- parse hunks;
- retain source spans for diagnostics;
- reject unsupported syntax;
- produce an immutable typed AST.

Suggested modules:

```text
src/protocol/mod.rs
src/protocol/lexer.rs
src/protocol/parser.rs
src/protocol/ast.rs
```

Output model:

```rust
struct PatchDocument {
    operations: Vec<FileOperation>,
}

enum FileOperation {
    Add(AddFile),
    Update(UpdateFile),
    Delete(DeleteFile),
}

struct AddFile {
    path: RepoPath,
    content: String,
}

struct UpdateFile {
    path: RepoPath,
    hunks: Vec<Hunk>,
}

struct DeleteFile {
    path: RepoPath,
}

struct Hunk {
    old_lines: Vec<HunkLine>,
    new_lines: Vec<HunkLine>,
    source_span: SourceSpan,
}
```

The exact representation may differ, but operation identity and source-location diagnostics must remain explicit.

#### 4.2.4 Path policy

Responsibilities:

- parse repository-relative paths;
- reject unsafe components;
- canonicalize the root;
- securely resolve parent directories;
- detect symlink escape;
- prohibit writes outside root;
- enforce per-path policy;
- reject directories when regular files are required.

Suggested module:

```text
src/path_policy.rs
```

All filesystem operations must accept `RepoPath`, not arbitrary `PathBuf`, after validation.

#### 4.2.5 Snapshot loader

Responsibilities:

- load metadata and bytes for every target;
- detect regular file, missing path, directory, or symlink;
- compute content fingerprint;
- record permissions and newline metadata;
- enforce file-size limits;
- detect binary or invalid UTF-8 content;
- construct immutable snapshots.

Suggested module:

```text
src/snapshot.rs
```

Model:

```rust
struct FileSnapshot {
    path: RepoPath,
    state: FileState,
}

enum FileState {
    Missing,
    Present(PresentFile),
}

struct PresentFile {
    bytes: Vec<u8>,
    text: String,
    fingerprint: ContentFingerprint,
    permissions: FilePermissions,
    newline_style: NewlineStyle,
    final_newline: bool,
    metadata_identity: MetadataIdentity,
}
```

#### 4.2.6 Semantic validator

Responsibilities:

- ensure operation/path state compatibility;
- reject duplicate paths;
- reject unsupported files;
- ensure update hunks are structurally valid;
- ensure operation count and byte limits;
- identify no-op patches.

Suggested module:

```text
src/validate.rs
```

#### 4.2.7 Patch planner

Responsibilities:

- pair every operation with its snapshot;
- produce an ordered mutation plan;
- detect path collisions;
- calculate intended final state;
- separate read phase from write phase;
- prepare commit metadata.

Suggested module:

```text
src/plan.rs
```

Model:

```rust
struct PatchPlan {
    root: CanonicalRoot,
    entries: Vec<PlannedChange>,
    base_fingerprints: BTreeMap<RepoPath, Option<ContentFingerprint>>,
}

enum PlannedChange {
    Create(PlannedCreate),
    Modify(PlannedModify),
    Remove(PlannedRemove),
}
```

#### 4.2.8 Patch engine

Responsibilities:

- apply update hunks entirely in memory;
- enforce deterministic matching;
- reject ambiguous matches;
- detect overlapping hunk effects;
- preserve unaffected content;
- produce proposed final bytes;
- calculate per-file diff summaries using `similar`;
- return no filesystem side effects.

Suggested modules:

```text
src/engine/mod.rs
src/engine/matcher.rs
src/engine/apply.rs
src/engine/diff_summary.rs
```

The engine may use `diffy` or `flickzeug`, but only behind a project-defined adapter.

The project contract must not inherit undocumented backend behavior accidentally.

Define an internal trait:

```rust
trait HunkApplier {
    fn apply(
        &self,
        base: &str,
        hunks: &[Hunk],
    ) -> Result<AppliedText, PatchApplyError>;
}
```

Provide contract tests independent of the backend implementation.

#### 4.2.9 Commit coordinator

Responsibilities:

- re-read or re-stat affected paths immediately before commit;
- compare current state with base fingerprints;
- abort on concurrent modification;
- prepare temporary files;
- fsync temp files where configured;
- commit mutations in a deterministic order;
- rollback already-committed changes on later failure;
- report rollback failure distinctly;
- ensure `--check` bypasses all write operations.

Suggested module:

```text
src/commit.rs
```

This is the highest-risk component and must have dedicated integration and fault-injection tests.

#### 4.2.10 Filesystem adapter

Responsibilities:

- abstract filesystem reads, metadata, temp creation, rename, deletion, chmod, sync;
- support real and fault-injected implementations;
- avoid direct `std::fs` use outside this layer.

Suggested module:

```text
src/fs.rs
```

Trait sketch:

```rust
trait FileSystem {
    fn read(&self, path: &Path) -> Result<Vec<u8>, FsError>;
    fn metadata(&self, path: &Path) -> Result<FileMetadata, FsError>;
    fn symlink_metadata(&self, path: &Path) -> Result<FileMetadata, FsError>;
    fn create_temp_near(&self, path: &Path) -> Result<TempFile, FsError>;
    fn rename(&self, from: &Path, to: &Path) -> Result<(), FsError>;
    fn remove_file(&self, path: &Path) -> Result<(), FsError>;
    fn set_permissions(&self, path: &Path, permissions: FilePermissions)
        -> Result<(), FsError>;
    fn sync_file(&self, path: &Path) -> Result<(), FsError>;
    fn sync_dir(&self, path: &Path) -> Result<(), FsError>;
}
```

#### 4.2.11 Diagnostics

Responsibilities:

- convert typed failures into stable public diagnostics;
- include operation index, path, hunk index, and source span;
- produce human and JSON representations;
- include one precise next-action hint;
- avoid stack traces unless explicitly enabled for development.

Suggested module:

```text
src/diagnostics.rs
```

#### 4.2.12 Instrumentation

Responsibilities:

- collect operation counts and timings;
- collect no sensitive content by default;
- support structured diagnostic events;
- expose development tracing through an environment variable;
- keep default output clean.

Suggested module:

```text
src/telemetry.rs
```

### 4.3 Boundary rules

1. CLI code may depend on application services, never the reverse.

2. Protocol parsing may not access the filesystem.

3. The patch engine may not perform filesystem writes.

4. Filesystem code may not interpret patch syntax.

5. Commit coordination may only receive a fully validated, fully applied plan.

6. No component may silently convert one operation type into another.

7. Backends such as `diffy` and `similar` remain behind adapters.

8. Public error codes may not expose crate-specific error strings.

9. Path safety must be centralized; no component may independently join untrusted strings to the root.

10. All file mutations flow through the commit coordinator.

---

## 5. Core Invariants

### I1 — Root confinement

Every read and write path resolves beneath the configured root.

### I2 — Transactionality

A patch either commits all file operations or leaves the repository in its original state.

### I3 — No mutation before validation

No target file is modified until the complete patch has been parsed, validated, loaded, and applied in memory.

### I4 — No silent fallback

A failed localized update never becomes a whole-file rewrite, fuzzy match, or first-match selection.

### I5 — Unique matching

Every update hunk must identify exactly one location in the current snapshot.

### I6 — State-operation compatibility

- Add requires missing target.
- Update requires existing regular text file.
- Delete requires existing regular file.

### I7 — Concurrent modification protection

The committed file state must derive from the same base state that was validated.

### I8 — Unaffected-content preservation

Bytes outside applied hunk ranges remain unchanged.

### I9 — Stable diagnostics

Equivalent failures produce the same public error code and exit class.

### I10 — Machine-pure JSON

With `--json`, stdout contains exactly one valid JSON document and no incidental text.

### I11 — Bounded resources

Patch size, file size, file count, and operation count are limited.

### I12 — Deterministic ordering

Parsing, planning, commit ordering, diagnostics, and JSON arrays use deterministic ordering.

### I13 — No path aliasing

Two syntactically distinct paths that resolve to the same filesystem object are rejected as a collision.

### I14 — No symlink escape

A path passing through a symlink outside the root is rejected.

### I15 — Validation parity

`--check` executes the same parse, validate, snapshot, plan, and apply phases as normal mode.

### I16 — Atomic visibility per file

Modified files are replaced through same-directory temporary files and atomic rename where the platform supports it.

### I17 — Explicit unsupported inputs

Binary, oversized, invalid UTF-8, special files, and unsupported protocol features fail explicitly.

### I18 — No hidden repository dependency

The tool works without Git, except for optional verification helpers outside the core runtime.

---

## 6. Data Flow

### 6.1 Normal apply flow

```text
Patch bytes
  ↓
Input limit check
  ↓
Protocol parse
  ↓
Path normalization and policy validation
  ↓
Operation-state validation
  ↓
Snapshot affected paths
  ↓
Compute fingerprints
  ↓
Apply all operations in memory
  ↓
Compute resulting diff summaries
  ↓
Revalidate current fingerprints
  ↓
Prepare temporary files and rollback material
  ↓
Commit mutations
  ↓
Verify committed states
  ↓
Emit result
```

### 6.2 Check-only flow

```text
Patch bytes
  ↓
Parse
  ↓
Validate
  ↓
Snapshot
  ↓
Apply in memory
  ↓
Compute summaries
  ↓
Return success
```

No temporary files, writes, renames, deletes, or chmod calls are permitted in check mode.

### 6.3 Failure flow

```text
Failure occurs
  ↓
Map internal error to public code
  ↓
Attach path / operation / hunk context
  ↓
Attach stable hint
  ↓
Ensure no uncommitted temporary state remains
  ↓
Rollback if commit started
  ↓
Emit JSON or human diagnostic
  ↓
Return stable exit code
```

---

## 7. Data Model and Schemas

### 7.1 JSON success schema

Example:

```json
{
  "version": 1,
  "ok": true,
  "mode": "apply",
  "root": "/workspace/project",
  "summary": {
    "files_total": 2,
    "files_added": 0,
    "files_updated": 2,
    "files_deleted": 0,
    "hunks_applied": 3,
    "lines_added": 7,
    "lines_deleted": 4,
    "duration_ms": 14
  },
  "files": [
    {
      "path": "src/config.rs",
      "operation": "update",
      "hunks": 1,
      "lines_added": 1,
      "lines_deleted": 1,
      "before_sha256": "…",
      "after_sha256": "…"
    }
  ]
}
```

### 7.2 JSON error schema

```json
{
  "version": 1,
  "ok": false,
  "error": {
    "code": "HUNK_NOT_FOUND",
    "exit_code": 1,
    "message": "Update hunk 2 did not match the current file.",
    "path": "src/config.rs",
    "operation_index": 1,
    "hunk_index": 2,
    "source": {
      "line": 14,
      "column": 1
    },
    "hint": "Read the current affected region and regenerate the patch from current content."
  }
}
```

### 7.3 Internal fingerprint model

Use SHA-256 or BLAKE3 over exact file bytes.

Fingerprint identity must include:

- existence state;
- exact content bytes;
- optional metadata identity if needed for race detection.

Recommended:

```rust
struct BaseIdentity {
    exists: bool,
    content_hash: Option<[u8; 32]>,
    size: Option<u64>,
    modified_time: Option<SystemTime>,
    inode_or_file_id: Option<FileId>,
}
```

Content hash remains authoritative. Metadata helps detect changes cheaply but must not replace content verification when committing.

### 7.4 Newline model

```rust
enum NewlineStyle {
    Lf,
    CrLf,
    Mixed,
    None,
}
```

v1 behavior:

- preserve LF;
- preserve CRLF;
- reject mixed line endings for update operations unless tests prove safe behavior;
- allow creation with LF only unless the protocol later supports explicit line endings.

### 7.5 File policy model

```rust
struct Limits {
    max_patch_bytes: usize,
    max_file_bytes: usize,
    max_files: usize,
    max_hunks_per_file: usize,
    max_total_hunks: usize,
}
```

Defaults:

```text
max_patch_bytes      4 MiB
max_file_bytes       16 MiB
max_files            128
max_hunks_per_file   256
max_total_hunks      2,048
```

These defaults are intentionally conservative for agent-generated source changes.

---

## 8. Filesystem Transaction Strategy

### 8.1 Required behavior

The implementation must prevent partial repository mutation where reasonably possible.

### 8.2 Preparation phase

For every create or update:

1. Create a temporary file in the target file’s parent directory.
2. Write proposed bytes.
3. Flush userspace buffers.
4. Optionally call `sync_all`.
5. Apply intended permissions.
6. Retain the temporary path without renaming.

For every update or delete:

1. Preserve rollback material:
   - original bytes in memory for files within the configured size bound; or
   - a same-directory backup file.

2. Preserve permissions.

### 8.3 Revalidation phase

Immediately before the first visible mutation:

1. Re-read or rehash every existing affected path.
2. Verify missing paths remain missing.
3. Verify existing paths remain identical to snapshots.
4. Abort with `CONCURRENT_MODIFICATION` if any differ.
5. Delete all prepared temporary files.

### 8.4 Commit ordering

Recommended deterministic order:

1. Deletes that unblock add-path collisions only if move support exists.
2. Updates.
3. Adds.
4. Remaining deletes.

Without move support and duplicate paths, use lexicographic repository-relative path order.

### 8.5 Rollback

If any commit operation fails after visible mutation begins:

1. Stop applying remaining operations.
2. Restore committed updates from rollback material.
3. Remove committed adds.
4. Restore committed deletes.
5. Verify restored fingerprints where possible.
6. Remove temporary files.
7. Return:
   - original failure if rollback succeeds;
   - `ROLLBACK_FAILED` / exit `6` if rollback is incomplete.

The diagnostic must list every path whose state could not be restored.

### 8.6 Honest atomicity claim

Do not claim global filesystem transactionality equivalent to a database transaction.

Document the guarantee as:

> `agent-patch` validates all operations before mutation and performs rollback on commit failure. Per-file replacement is atomic on supported filesystems. Multi-file atomic visibility is not guaranteed by ordinary filesystems, but partial commits are actively rolled back and surfaced as hard errors.

---

## 9. Matching and Patch Application

### 9.1 Parsing model

Do not pass unvalidated raw patch text directly to `diffy`.

First parse the project protocol into a typed AST.

Then translate validated update hunks into backend operations.

### 9.2 Hunk application

For each update file:

1. Start from its immutable snapshot.
2. Apply hunks in source order.
3. Track applied byte or line ranges.
4. Reject overlapping or contradictory hunks.
5. Require each hunk to match uniquely.
6. Return final text only after all hunks succeed.
7. Compare base and final output.
8. Reject no-effect updates unless an explicit future flag permits them.

### 9.3 Backend adapter tests

Regardless of whether `diffy` or `flickzeug` is used, contract tests must cover:

- hunk line-number drift;
- unique context relocation;
- ambiguous repeated blocks;
- missing context;
- adjacent hunks;
- overlapping hunks;
- CRLF preservation;
- final newline changes;
- empty-file update;
- Unicode content;
- context containing patch marker-like text.

### 9.4 Result diff

Use `similar` to calculate:

- inserted line count;
- deleted line count;
- changed ranges;
- optional unified diff for verbose diagnostics;
- unexpected broad-churn ratio.

Do not use the generated diff to determine correctness. It is observational output after the intended result has been constructed.

---

## 10. Error Handling

### 10.1 Philosophy

Every error must answer:

1. What failed?
2. Where did it fail?
3. Why was no mutation performed, or what rollback occurred?
4. What should the caller do next?

No component may swallow an error and proceed using weaker semantics.

### 10.2 Typed errors

Recommended hierarchy:

```rust
enum AppError {
    Input(InputError),
    Parse(ParseError),
    Policy(PolicyError),
    Snapshot(SnapshotError),
    Apply(ApplyError),
    Concurrency(ConcurrencyError),
    Commit(CommitError),
    Internal(InternalError),
}
```

Every public error maps to:

```rust
struct PublicError {
    code: ErrorCode,
    exit_code: u8,
    message: String,
    path: Option<RepoPath>,
    operation_index: Option<usize>,
    hunk_index: Option<usize>,
    source_span: Option<SourceSpan>,
    hint: Option<String>,
}
```

### 10.3 Retriable versus non-retriable

The CLI itself performs no blind retries for semantic failures.

Non-retriable within the same invocation:

- malformed patch;
- unsafe path;
- unsupported file;
- ambiguous hunk;
- hunk not found;
- duplicate operation;
- file state mismatch.

Potentially retriable internally:

- interrupted system call;
- transient temporary-file creation failure;
- metadata read interrupted by signal.

Internal retry rules:

- retry only explicitly recognized transient OS errors;
- maximum 2 retries;
- bounded delay:
  - first retry immediately;
  - second retry after 10 ms;

- no retry after visible mutation without entering rollback;
- record retry count in debug tracing.

Concurrent modification is not internally retried. The caller must reread and regenerate the patch.

### 10.4 No silent fallback examples

Forbidden:

```text
Exact hunk not found → replace the whole file
```

Forbidden:

```text
Two matches found → choose the first
```

Forbidden:

```text
CRLF handling failed → normalize to LF
```

Forbidden:

```text
Atomic rename failed → copy over the destination
```

Forbidden:

```text
JSON serialization failed → print human output to stdout
```

Required behavior is to fail with a stable diagnostic.

---

## 11. Performance Targets

### 11.1 Baseline workload

Target repository-local patch:

- 1–10 files;
- 1–20 hunks;
- affected files under 1 MiB each;
- patch under 256 KiB.

### 11.2 Latency targets

On a typical developer laptop with warm filesystem cache:

- startup and parse:
  - p50 under 8 ms;
  - p95 under 20 ms.

- `--check`, 1 file / 1 hunk / 100 KiB:
  - p50 under 10 ms;
  - p95 under 30 ms.

- apply, 10 files / 20 hunks / 5 MiB total:
  - p50 under 40 ms;
  - p95 under 120 ms.

- apply, maximum default patch size:
  - p95 under 500 ms excluding filesystem sync latency.

### 11.3 Memory targets

- steady overhead under 10 MiB for small patches;
- peak resident memory under:
  - `3 × total affected input bytes + 32 MiB`;

- never load unrelated repository files;
- fail before allocation when declared resource limits are exceeded.

### 11.4 Scaling targets

Complexity targets:

- parsing: O(patch bytes);
- path validation: O(number of path components);
- snapshot load: O(total affected bytes);
- diff generation: bounded according to `similar` algorithm; benchmark repetitive worst-case inputs;
- hunk matching: avoid unbounded quadratic scans across large repetitive files.

For pathological repeated input, enforce time or work limits rather than hanging.

### 11.5 Performance gates

CI benchmark warnings:

- fail only on gross regression initially;
- flag:
  - more than 25% latency regression;
  - more than 25% allocation regression;
  - more than 2× worst-case matching time.

Use Criterion benchmarks, but keep performance tests separate from correctness tests.

---

## 12. Instrumentation Plan

### 12.1 Default behavior

No telemetry leaves the machine.

No source content is logged by default.

No file contents appear in diagnostics unless the user explicitly requests verbose development output.

### 12.2 Timers

Record durations for:

```text
input_read_ms
parse_ms
path_validation_ms
snapshot_ms
apply_ms
revalidation_ms
commit_prepare_ms
commit_ms
rollback_ms
total_ms
```

### 12.3 Counters

Record:

```text
patch_bytes
operation_count
file_count
hunk_count
input_bytes
output_bytes
lines_added
lines_deleted
temp_files_created
filesystem_reads
filesystem_writes
internal_retry_count
```

### 12.4 Debug tracing

Environment variable:

```bash
AGENT_PATCH_LOG=debug
```

Optional values:

```text
error
warn
info
debug
trace
```

Logging format:

- structured key-value text by default;
- optional JSON logs only through a future explicit flag;
- stderr only.

Every operation receives a short invocation ID.

Example:

```text
level=debug invocation=7f4a phase=snapshot path=src/config.rs bytes=4821 duration_ms=1
```

Never log patch contents at `info` or below.

### 12.5 Metrics output

Do not add Prometheus or daemon metrics in v1.

The JSON success object provides sufficient per-invocation instrumentation.

### 12.6 Diagnostic correlation

Include:

- invocation ID;
- operation index;
- hunk index;
- repository-relative path.

Do not include user names, absolute paths in default human output, or source content unnecessarily.

---

## 13. Security Model

### 13.1 Threats

Assume patch input may be malformed or adversarial.

Threats include:

- path traversal;
- symlink escape;
- overwriting arbitrary host files;
- special-file writes;
- resource exhaustion;
- malformed Unicode;
- ambiguous context causing unintended edits;
- TOCTOU modification;
- patch marker injection;
- temporary-file races;
- accidental secret exposure in logs;
- decompression or parser bombs if future formats are added.

### 13.2 Required controls

1. Canonicalize the root once.
2. Reject absolute paths.
3. Reject `..`.
4. Resolve and inspect every existing ancestor.
5. Reject symlink traversal outside root.
6. Reject special files:
   - device;
   - socket;
   - FIFO;
   - directory.

7. Use unpredictable temporary names.
8. Create temporary files with exclusive creation.
9. Use same-directory temp files.
10. Enforce patch and file-size bounds.
11. Avoid shelling out from the Rust runtime.
12. Avoid interpreting file content as commands.
13. Keep diagnostics content-bounded.
14. Fuzz the parser.
15. Test malicious path encodings and Unicode edge cases.

---

## 14. Test Plan

### 14.1 Unit tests

#### Parser

Cover:

- valid add;
- valid update;
- valid delete;
- multi-file patch;
- multiple hunks;
- missing begin marker;
- missing end marker;
- nested begin marker;
- trailing content;
- unknown operation;
- empty path;
- malformed hunk line;
- duplicate end marker;
- marker-like text inside added content;
- empty patch;
- no-op hunk;
- Unicode paths where supported;
- invalid UTF-8 patch input;
- limit boundaries.

Every parser failure test must assert:

- public error code;
- source line;
- source column where available;
- no panic;
- no partial AST.

#### Path policy

Cover:

- normal relative paths;
- `..`;
- absolute Unix paths;
- Windows drive-like paths even on Unix;
- repeated separators;
- `.` segments;
- symlink within root;
- symlink outside root;
- symlinked parent;
- path alias collision;
- directory target;
- FIFO or socket where platform supports fixtures;
- nonexistent parent;
- case-only collision behavior on case-insensitive filesystems where testable.

#### Hunk matching

Cover:

- exact match;
- shifted line position;
- repeated unique block with wider context;
- ambiguous block;
- missing block;
- adjacent hunks;
- overlapping hunks;
- hunk order dependence;
- insertion at start;
- insertion at end;
- deletion of full file contents;
- empty file;
- CRLF;
- no final newline;
- Unicode graphemes;
- tabs and spaces remain distinct;
- whitespace-only lines;
- extremely long lines;
- marker text within source.

#### Snapshot

Cover:

- missing file;
- regular file;
- oversized file;
- invalid UTF-8;
- UTF-8 BOM;
- CRLF;
- mixed newline;
- no final newline;
- permissions capture;
- content hash stability.

#### Diff summary

Cover:

- line additions;
- line deletions;
- replacement;
- empty-to-content;
- content-to-empty;
- no change;
- CRLF preservation;
- large repeated input.

#### Diagnostics

Snapshot-test:

- human messages;
- JSON schema;
- path and hunk fields;
- stable hints;
- absence of ANSI in JSON;
- absence of source content for sensitive failures;
- exact exit mapping.

### 14.2 Property tests

Use `proptest` for:

1. Parser never panics on arbitrary byte input.
2. Unsafe path inputs never escape the root.
3. Successful apply followed by diff confirms only intended output.
4. Update application is deterministic.
5. JSON output always parses.
6. Check mode never invokes mutating filesystem operations.
7. Equivalent line-ending round trips preserve exact bytes.
8. Random failed patches do not modify filesystem state.
9. Add then inverse delete restores original tree in controlled fixtures.
10. Generated non-overlapping hunks apply in source order consistently.

### 14.3 Integration tests

Run against real temporary directories.

#### Core apply

- apply one update;
- apply add/update/delete together;
- validate all final bytes;
- validate permissions;
- validate output;
- validate exit code.

#### Atomic prevalidation

Construct a patch where:

- first operation is valid;
- second operation fails.

Assert:

- neither path changed;
- no temporary files remain;
- error references second operation.

#### Concurrent modification

Use a test synchronization hook:

1. snapshot file;
2. pause before commit;
3. modify file externally;
4. resume.

Assert:

- exit code `5`;
- no intended patch applied;
- external modification remains intact;
- diagnostic code is `CONCURRENT_MODIFICATION`.

#### Commit failure and rollback

Use fault-injected filesystem adapter:

- fail second rename;
- fail delete;
- fail chmod;
- fail temp-file flush;
- fail rollback restore.

Assert exact postconditions for each case.

#### Check mode

Assert:

- successful plan;
- identical diagnostics to apply mode before commit;
- zero writes;
- zero temp files;
- unchanged file tree.

#### Root confinement

Attempt:

- `../outside`;
- absolute path;
- symlink to outside;
- symlinked intermediate directory.

Assert no outside file is read or written.

#### Resource limits

Test:

- exact limit accepted;
- one byte over rejected;
- too many files;
- too many hunks;
- oversized target file.

#### CLI input modes

Test:

- stdin;
- patch file;
- missing patch file;
- both malformed combinations if applicable;
- empty stdin;
- broken pipe behavior;
- JSON mode;
- quiet mode.

### 14.4 End-to-end tests

Run the compiled release binary.

#### E2E-1 — Canonical agent invocation

Execute:

```bash
agent-patch <<'PATCH'
...
PATCH
```

Assert:

- exit `0`;
- concise stdout;
- expected repository diff;
- `git diff --check` succeeds.

#### E2E-2 — User-supplied multi-file patch

Apply a realistic patch across:

- Rust source;
- TOML configuration;
- Markdown documentation.

Assert exact file contents and no unrelated churn.

#### E2E-3 — Ambiguous target

Use repeated code blocks.

Assert:

- exit `1`;
- code `HUNK_AMBIGUOUS`;
- no mutation;
- hint tells the agent to read more context.

#### E2E-4 — Stale patch

Generate patch from old content, modify target, apply.

Assert:

- exit `1` or `5`, depending on when drift is detected;
- no overwrite;
- external content preserved.

#### E2E-5 — Malicious paths

Use path traversal and symlink escape.

Assert:

- exit `4`;
- no outside access;
- stable JSON error.

#### E2E-6 — Large but valid patch

Use:

- 100 files;
- 1,000 total hunks;
- near configured byte limits.

Assert target latency and memory budgets.

#### E2E-7 — Crash recovery simulation

Inject process termination between commit steps using a test-only binary build.

Verify:

- repository state is either original or explicitly recoverable;
- no silent success;
- leftover temp/backup files are recognizable and bounded.

A future `recover` command may be added if crash-recovery artifacts are retained. Do not add one in v1 unless crash testing proves it necessary.

### 14.5 Logging expectations in tests

Every failing integration and E2E test must capture:

- command invocation;
- working directory;
- exit status;
- stdout;
- stderr;
- pre-operation tree digest;
- post-operation tree digest;
- per-file hashes for affected paths;
- leftover temporary files;
- elapsed time.

On success, routine CI logs should stay compact.

On failure, test helpers should print:

```text
=== invocation ===
...

=== exit ===
...

=== stdout ===
...

=== stderr ===
...

=== before tree ===
...

=== after tree ===
...

=== affected file diffs ===
...
```

Do not rely on assertion messages that show only “left != right.”

### 14.6 Fuzzing

Add cargo-fuzz targets for:

- protocol parser;
- path parser;
- hunk translator;
- update application;
- JSON diagnostics serialization.

Fuzz invariants:

- no panic;
- no out-of-root path;
- no uncontrolled allocation;
- no mutation during parse;
- deterministic result for repeated input.

---

## 15. Repository Structure

```text
agent-patch/
├── Cargo.toml
├── Cargo.lock
├── README.md
├── LICENSE
├── CLAUDE.md
├── AGENTS.md
├── crates/
│   └── agent-patch/
│       ├── Cargo.toml
│       ├── src/
│       │   ├── main.rs
│       │   ├── cli.rs
│       │   ├── app.rs
│       │   ├── input.rs
│       │   ├── diagnostics.rs
│       │   ├── telemetry.rs
│       │   ├── limits.rs
│       │   ├── path_policy.rs
│       │   ├── snapshot.rs
│       │   ├── validate.rs
│       │   ├── plan.rs
│       │   ├── commit.rs
│       │   ├── fs.rs
│       │   ├── protocol/
│       │   │   ├── mod.rs
│       │   │   ├── ast.rs
│       │   │   ├── lexer.rs
│       │   │   └── parser.rs
│       │   └── engine/
│       │       ├── mod.rs
│       │       ├── matcher.rs
│       │       ├── apply.rs
│       │       └── diff_summary.rs
│       ├── tests/
│       │   ├── cli.rs
│       │   ├── atomicity.rs
│       │   ├── concurrency.rs
│       │   ├── path_safety.rs
│       │   ├── limits.rs
│       │   └── fixtures/
│       └── benches/
│           ├── parser.rs
│           ├── matcher.rs
│           └── apply.rs
├── scripts/
│   ├── agent-patch
│   ├── test
│   ├── lint
│   └── bench
├── docs/
│   ├── protocol.md
│   ├── errors.md
│   ├── architecture.md
│   └── threat-model.md
└── fuzz/
    ├── Cargo.toml
    └── fuzz_targets/
```

---

## 16. Parallel Agent Work Plan

### 16.1 Workstream boundaries

Each parallel agent owns a non-overlapping module group.

#### Agent A — Protocol

Owns:

```text
protocol/*
docs/protocol.md
parser unit tests
parser fuzz target
```

Contract:

- no filesystem dependencies;
- outputs typed AST;
- stable source spans;
- no public diagnostics formatting.

#### Agent B — Path safety and snapshots

Owns:

```text
path_policy.rs
snapshot.rs
path-safety tests
snapshot tests
docs/threat-model.md path sections
```

Contract:

- outputs validated `RepoPath`;
- all target states represented explicitly;
- no patch application.

#### Agent C — Patch engine

Owns:

```text
engine/*
engine tests
matcher benchmarks
backend adapter evaluation
```

Contract:

- pure in-memory operation;
- deterministic matching;
- no filesystem;
- no CLI or JSON concerns.

#### Agent D — Planning and validation

Owns:

```text
validate.rs
plan.rs
limits.rs
validation tests
```

Contract:

- consumes AST plus snapshots;
- outputs complete `PatchPlan`;
- no visible mutation.

#### Agent E — Filesystem and commit

Owns:

```text
fs.rs
commit.rs
atomicity tests
fault-injection filesystem
concurrency tests
```

Contract:

- accepts only validated plans;
- owns every mutation;
- implements rollback.

#### Agent F — CLI and diagnostics

Owns:

```text
main.rs
cli.rs
app.rs
input.rs
diagnostics.rs
telemetry.rs
docs/errors.md
CLI integration tests
```

Contract:

- no patch semantics;
- stable output and exits;
- JSON schema ownership.

#### Agent G — E2E, packaging, and repository integration

Owns:

```text
scripts/*
README.md
CLAUDE.md
AGENTS.md
E2E tests
CI
release packaging
```

Contract:

- does not alter internal APIs without coordination;
- validates built binary as an external consumer.

### 16.2 Shared contracts to freeze first

Before parallel implementation, freeze:

1. Patch grammar.
2. AST types.
3. Public error codes.
4. Exit codes.
5. `RepoPath` API.
6. Snapshot API.
7. `PatchPlan` API.
8. Filesystem trait.
9. JSON schema.
10. Default limits.
11. Matching semantics.
12. Concurrency doctrine.
13. Commit and rollback guarantees.

Place frozen contracts in:

```text
docs/contracts/
```

or in a single:

```text
docs/contract-v1.md
```

No agent may change a frozen field, enum variant, exit code, or invocation form without an explicit contract change.

### 16.3 Integration order

```text
Phase 0: freeze contracts
Phase 1: parser + path + diagnostics skeletons
Phase 2: snapshots + validation + pure engine
Phase 3: planner + check-only application
Phase 4: commit coordinator + rollback
Phase 5: CLI integration
Phase 6: security and fuzzing
Phase 7: performance and packaging
Phase 8: dogfood with coding agents
```

---

## 17. Implementation Phases and Acceptance Criteria

### Phase 0 — Contract freeze

Deliver:

- protocol document;
- error taxonomy;
- exit codes;
- JSON schema;
- operation semantics;
- matching semantics;
- resource limits;
- root and symlink policy.

Acceptance:

- all examples parse unambiguously;
- no open semantic questions remain;
- parallel agents can implement without guessing.

### Phase 1 — Parser and diagnostics

Deliver:

- complete parser;
- source spans;
- typed errors;
- human and JSON diagnostics.

Acceptance:

- malformed corpus tests pass;
- parser fuzz target runs for at least 10 million cases without crash;
- JSON output validates against checked-in schema.

### Phase 2 — Safe snapshot and pure apply

Deliver:

- path confinement;
- file snapshots;
- pure patch engine;
- diff summaries.

Acceptance:

- no filesystem writes;
- ambiguous matching rejected;
- exact content preservation verified;
- CRLF and final-newline tests pass or explicitly reject unsupported cases.

### Phase 3 — Check mode

Deliver:

- full CLI `--check`;
- parse-to-plan pipeline;
- result summaries.

Acceptance:

- zero mutating filesystem calls;
- realistic patches validate;
- stale and malformed patches fail correctly;
- latency target met.

### Phase 4 — Commit and rollback

Deliver:

- temp-file preparation;
- revalidation;
- update/add/delete commit;
- rollback;
- fault injection.

Acceptance:

- no partial mutation across all injected single-failure points;
- concurrent modification detected;
- rollback failure distinctly reported;
- no leftover temp files after ordinary failures.

### Phase 5 — Release CLI

Deliver:

- final options;
- repo-local wrapper;
- documentation;
- stable exit behavior.

Acceptance:

```bash
scripts/agent-patch --check < fixture.patch
scripts/agent-patch < fixture.patch
scripts/agent-patch --json < fixture.patch
```

all behave exactly as documented.

### Phase 6 — Security hardening

Deliver:

- path traversal corpus;
- symlink tests;
- resource-limit tests;
- parser fuzzing;
- filesystem fuzz or model tests where feasible.

Acceptance:

- no out-of-root read or write;
- no panics;
- no unbounded memory behavior on adversarial input;
- threat model reviewed.

### Phase 7 — Performance

Deliver:

- Criterion suite;
- benchmark fixture generator;
- baseline numbers;
- regression thresholds.

Acceptance:

- performance targets met on Linux reference machine;
- pathological repetitive-input behavior bounded;
- release binary size documented.

### Phase 8 — Agent dogfooding

Run at least three agent environments:

- Claude Code;
- Codex or equivalent patch-familiar agent;
- generic shell-capable coding agent.

Tasks:

1. Single localized replacement.
2. Multi-hunk same-file change.
3. Multi-file coherent change.
4. User-supplied patch.
5. Stale patch recovery.
6. Ambiguous target recovery.
7. Add and delete operations.

Collect:

- correct tool-selection rate;
- malformed patch rate;
- successful first-apply rate;
- retries per task;
- accidental whole-file rewrite attempts;
- diagnostic usefulness;
- token output compared with whole-file replacement.

Acceptance targets:

```text
first-apply success rate            ≥ 90%
malformed patch rate                ≤ 3%
unsafe path attempts accepted       0
silent partial application          0
whole-file rewrite fallback         0
agent recovery after stale patch    ≥ 90%
```

---

## 18. Verification Commands

Minimum local gate:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo test --workspace --release
```

Security and property gate:

```bash
cargo test --test path_safety
cargo test --test atomicity
cargo test --test concurrency
cargo test --test limits
```

Fuzz smoke gate:

```bash
cargo fuzz run parser -- -max_total_time=60
cargo fuzz run path_parser -- -max_total_time=60
cargo fuzz run hunk_apply -- -max_total_time=60
```

Benchmark gate:

```bash
cargo bench
```

End-to-end gate:

```bash
scripts/test-e2e
```

Manual smoke:

```bash
tmp="$(mktemp -d)"
cd "$tmp"
git init -q

mkdir -p src
cat > src/config.rs <<'EOF'
pub const RETRIES: usize = 2;
pub const TIMEOUT_SECS: u64 = 30;
EOF

/path/to/agent-patch <<'PATCH'
*** Begin Patch
*** Update File: src/config.rs
@@
 pub const RETRIES: usize = 2;
-pub const TIMEOUT_SECS: u64 = 30;
+pub const TIMEOUT_SECS: u64 = 45;
*** End Patch
PATCH

git diff --check
git diff -- src/config.rs
```

Expected:

- exit `0`;
- only timeout line changes;
- no temporary files;
- no formatting churn.

---

## 19. Claude Code and Agent Instructions

Recommended repository instruction:

````markdown
## Localized file editing

Use the native exact-replacement edit operation for one small, unique
replacement.

Use `scripts/agent-patch` when:

- a change contains multiple related hunks;
- several files should change atomically;
- additions or removals are clearer as contextual hunks;
- the user supplied a patch;
- exact replacement would require copying a large unchanged block.

Invoke it with a single-quoted heredoc:

```bash
scripts/agent-patch <<'PATCH'
*** Begin Patch
*** Update File: path/to/file
@@
-old text
+new text
*** End Patch
PATCH
```
````

For a nontrivial patch, validate first:

```bash
scripts/agent-patch --check < /tmp/change.patch
```

When a patch fails, read the current affected region and regenerate the patch.
Do not recover by overwriting the entire existing file.

Do not guess unsupported flags. Run:

```bash
scripts/agent-patch --help
```

````

Keep this directive short. The executable’s diagnostics should carry most recovery guidance.

---

## 20. Open Decisions to Resolve Before Coding

The following must be explicitly decided during Phase 0:

1. Use `diffy`, `flickzeug`, or a custom matcher.
2. Whether context reduction is supported.
3. Whether mixed line endings are rejected or preserved.
4. Whether UTF-8 BOM is preserved.
5. Whether delete operations require file-content confirmation.
6. Whether rename/move is v1 or deferred.
7. Whether empty-file deletion is allowed.
8. Whether no-op patches succeed or fail.
9. Whether executable permissions on newly added files can be expressed.
10. Whether commit uses in-memory rollback material or backup files.
11. Whether directory creation is implicit for `Add File`.
12. Whether parent directories created during a failed transaction are removed.
13. Whether absolute root paths appear in JSON.
14. Whether content hashes are SHA-256 or BLAKE3.
15. Whether fsync is default, optional, or platform-dependent.

Recommended v1 decisions:

```text
backend                 adapter over diffy/flickzeug, contract-tested
context reduction       supported only if unique and deterministic
mixed line endings      reject updates
UTF-8 BOM               preserve
delete confirmation     path-state only
rename                   defer
no-op patch              fail
executable add mode      defer
rollback storage         in memory under file-size limit
directory creation      explicit and transactional for Add File
absolute paths in JSON   only when explicitly requested
hash                     BLAKE3 internally; label algorithm explicitly
fsync                    enabled for temp files, configurable later
````

---

## 21. Definition of Done

The project is complete for v1 when:

1. The documented patch protocol is frozen and versioned.
2. All supported operations apply deterministically.
3. Malformed, stale, ambiguous, unsafe, and unsupported patches fail closed.
4. No validation failure modifies the filesystem.
5. Commit failures trigger tested rollback.
6. Concurrent modifications are never overwritten.
7. JSON output is stable and machine-readable.
8. Exit codes are stable and documented.
9. Linux and macOS CI pass.
10. Parser, path, and hunk fuzz targets run cleanly.
11. Performance targets are met.
12. Real coding agents successfully use the repo-local binary through `Bash`.
13. The agent instruction fits in a compact `CLAUDE.md` section.
14. No MCP server or harness-specific registration is required.
15. No whole-file rewrite fallback exists anywhere in the codebase.
16. The README enables a new user or coding agent to:
    - build;
    - validate a patch;
    - apply a patch;
    - understand a failure;
    - verify the resulting diff;
      within ten minutes.
