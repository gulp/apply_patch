# Patch Protocol

Canonical V4A-family dialect for `agent-patch`. Baseline: [contract-v1.md](./contract-v1.md). Extensions (pins, plans, fuzzy): [contract-v2.md](./contract-v2.md).

## Envelope

```text
*** Begin Patch
<operations>
*** End Patch
```

Exactly one begin and one end. No non-whitespace content outside the envelope. At least one file operation is required.

## Add File

```text
*** Add File: path/to/file
+line one
+line two
```

Every content line must start with `+`. The target path must not exist. Parent directories are created as needed and rolled back if the transaction fails. Content lines are joined with LF (`\n`).

## Update File

```text
*** Update File: path/to/file
@@
 context
-old
+new
 context
```

Optional content-hash pin (checked against the on-disk file before locate):

```text
*** Update File: path/to/file
*** Hash: blake3 <64-hex-chars>
@@
-old
+new
```

`blake3 <hex>` and `blake3:<hex>` are accepted. Mismatch → `HASH_PIN_MISMATCH` (exit 5).

Hunks begin with `@@` or `@@ <anchor>`:

- Bare `@@` starts a section.
- `@@ <anchor>` advances the search cursor to a unique exact line equal to `<anchor>`, then locates the hunk body.
- Unified-diff numeric forms such as `@@ -1,3 +1,4 @@` are ignored as line-number math; the hunk body still matches by content.

Each hunk needs at least one `-` or `+` line. Context lines start with a single space. The target must be an existing regular UTF-8 text file.

### End of File

An optional trailing `*** End of File` marks EOF-prefer locate for that hunk:

```text
*** Update File: path/to/file
@@
 trailing context
-old
+new
*** End of File
```

Locate prefers an exact match aligned at the end of the file (`len - old_len`). If that fails, search continues with unique matching from the current cursor.

### Matching and newlines

Default matching is unique and exact on the file’s logical lines (line endings stripped for comparison). Apply locates all hunks on the original line array, then emits with a forward cursor. Controlled edge-context reduction applies only when the reduced needle is unique. Ambiguous or missing context fails (`HUNK_AMBIGUOUS` / `HUNK_NOT_FOUND`). Empty no-op hunks and no-effect updates fail (`PATCH_NO_EFFECT`).

Optional CLI `--fuzzy=rstrip|strip` extends the ladder after exact/context-reduction failure. Every enabled level still requires a unique hit; first-match-wins never applies. See [contract-v2.md](./contract-v2.md).

The file’s LF or CRLF style is preserved on emit. Mixed line endings on update are rejected. UTF-8 BOM is preserved when present.

## Delete File

```text
*** Delete File: path/to/file
```

The target must be an existing regular file. No content body. Empty files may be deleted.

## Paths

- Repository-relative UTF-8
- Absolute paths forbidden
- `.` and `..` components forbidden after normalization
- Empty paths and NUL bytes forbidden
- Duplicate operations on the same path forbidden
- Unknown headers are hard errors
- Symlink traversal that escapes `--root` is rejected

## Unsupported

- `*** Move File` / `*** Move to:` (see [design/move.md](./design/move.md))
- Binary files
- First-match-wins matching
- Executable-bit control on Add
