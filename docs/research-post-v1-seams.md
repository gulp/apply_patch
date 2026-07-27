# Ground truth: post-v1 plan seams

Probed 2026-07-27 via `opensrc` + grep-app against live upstream trees. Use this when implementing `IMPLEMENTATION_PLAN.md` Phases 1–8 so interfaces stay aligned with real code—not folklore.

## Cache paths

```bash
opensrc path openai/codex#main
opensrc path openai/openai-agents-python#main
opensrc path openai/openai-agents-js#main
opensrc path prefix-dev/flickzeug#main
```

| Source | Path under `~/.opensrc/repos/github.com/` |
| --- | --- |
| Codex apply-patch | `openai/codex/main/codex-rs/apply-patch/` |
| Agents Python | `openai/openai-agents-python/main/src/agents/apply_diff.py` |
| Agents JS | `openai/openai-agents-js/main/packages/agents-core/src/utils/applyDiff.ts` |
| flickzeug | `prefix-dev/flickzeug/main/src/apply.rs` |

---

## Seam: locate → emit (`ExecutionPlan` / pure planner)

### Agents Python (canonical chunk shape)

```python
@dataclass
class Chunk:
    orig_index: int
    del_lines: list[str]
    ins_lines: list[str]

def _apply_chunks(input: str, chunks: list[Chunk], newline: str) -> str:
    orig_lines = input.split("\n")
    dest_lines: list[str] = []
    cursor = 0
    for chunk in chunks:
        if cursor > chunk.orig_index:
            raise ValueError(...)  # overlap
        dest_lines.extend(orig_lines[cursor : chunk.orig_index])
        cursor = chunk.orig_index
        if chunk.ins_lines:
            dest_lines.extend(chunk.ins_lines)
        cursor += len(chunk.del_lines)
    dest_lines.extend(orig_lines[cursor:])
    return newline.join(dest_lines)
```

### Agents JS (same algorithm, different newline policy)

```typescript
export function applyDiff(
  input: string,
  diff: string,
  mode: 'default' | 'create' = 'default',
): string;

type Chunk = { origIndex: number; delLines: string[]; insLines: string[] };
```

JS always joins with `\n` after `split('\n')`. **Python detects file CRLF and restores it** — agent-patch follows Python.

### Codex (locate then replacements, not Agents-style emit)

```rust
pub(crate) fn seek_sequence(
    lines: &[String],
    pattern: &[String],
    start: usize,
    eof: bool,
) -> Option<usize>;

fn compute_replacements(
    original_lines: &[String],
    path: &str,
    chunks: &[UpdateFileChunk],
) -> Result<Vec<(usize, usize, Vec<String>)>, ApplyPatchError>;
```

Codex builds `(start, old_len, new_lines)` then applies replacements (descending). Functionally similar to locate→emit; Agents’ forward cursor is the clearer model for `locate_chunks` / `emit_chunks`.

**agent-patch:** keep Agents chunk model; freeze digest-bearing `ExecutionPlan` on top (no upstream equivalent).

---

## Seam: matching ladder + EOF + uniqueness

### Codex `seek_sequence` (first hit wins)

Order when searching from `search_start` (EOF sets start to `len - pattern.len()` first):

1. Exact `==`
2. `trim_end` both sides
3. `trim` both sides
4. Unicode punctuation/space normalisation → ASCII, then `trim`

Empty pattern → `Some(start)`. Pattern longer than file → `None`.

Call site also retries without a trailing empty old-line sentinel (final-newline artifact).

### Agents `_find_context_core` (first hit wins)

```python
def _find_context(lines, context, start, eof) -> ContextMatch:
    if eof:
        end_match = _find_context_core(lines, context, max(0, len(lines) - len(context)))
        if end_match.new_index != -1:
            return end_match
        fallback = _find_context_core(lines, context, start)
        return ContextMatch(fallback.new_index, fallback.fuzz + 10000)
    return _find_context_core(lines, context, start)

# core: exact (fuzz 0) → rstrip (1) → strip (100); first index wins
```

### flickzeug (unified-diff only — wrong dialect)

```rust
pub struct FuzzyConfig {
    pub max_fuzz: usize,           // default 2
    pub ignore_whitespace: bool,   // default false
    pub ignore_case: bool,         // default false
}
// FuzzyComparable::similarity uses Levenshtein; fuzzy_eq if similarity > 0.8
```

**Do not** call flickzeug/`diffy` on V4A text.

### agent-patch contract (deliberate deltas)

| Topic | Upstream | agent-patch |
| --- | --- | --- |
| Default | Silent fuzz ladder, first match | Unique exact only |
| Opt-in `--fuzzy` | Always on in Codex/Agents | `off\|rstrip\|strip`; **still unique** |
| Unicode normalize | Codex step 4 | Deferred (plan §9.2) |
| EOF | Prefer EOF start, then search | Prefer EOF exact, then unique forward |
| Ambiguity | Impossible (first wins) | `HUNK_AMBIGUOUS` + oracle candidates |

`MatchEvidence.accepted_level` maps: `exact` / `context_reduced` / `rstrip` / `strip` / `eof`. Do **not** map flickzeug `max_fuzz` or similarity scores into selection.

---

## Seam: already-applied / idempotence

| System | Interface | Meaning |
| --- | --- | --- |
| flickzeug | `is_diff_applied_with_config(base, &diff, &config) -> bool` | Unified-diff **reverse round-trip** |
| flickzeug | `ApplyOutcome::{Applied, AlreadyApplied, Failed}` | Check already-applied **before** forward apply |
| Codex | `AppliedPatchDelta::is_exact()` | Whether FS writes remain trustworthy after partial failure — **not** already-applied |
| agent-patch v1 | `PATCH_NO_EFFECT` | Emit equals base |
| agent-patch plan | `--idempotent` + `PARTIALLY_APPLIED` | Full intended after-state proof |

```rust
// flickzeug — observational only for our research; dialect is unified
pub fn is_diff_applied_with_config(
    base_image: &[u8],
    diff: &Diff<'_, [u8]>,
    config: &ApplyConfig,
) -> bool;

pub enum ApplyOutcome {
    Applied(Vec<u8>, ApplyStats),
    AlreadyApplied(Vec<u8>),
    Failed(ApplyError),
}
```

V4A has no reverse-diff primitive. Idempotence must be proven from locator + planned after bytes (plan §9.4), not by importing flickzeug.

---

## Seam: crash / partial failure (why journals exist)

Codex is **not** transactional. It records committed textual mutations:

```rust
pub struct AppliedPatchDelta {
    changes: Vec<AppliedPatchChange>,
    exact: bool,  // aggregate trust after append
}

pub enum AppliedPatchFileChange {
    Add { content: String, overwritten_content: Option<String> },
    Delete { content: String },
    Update {
        move_path: Option<PathBuf>,
        old_content: String,
        overwritten_move_content: Option<String>,
        new_content: String,
    },
}
```

Scenario `015_failure_after_partial_success_leaves_changes` expects earlier ops to remain. **agent-patch** rejects that product shape: durable journal + before-image CAS + `recover` (see [design/transaction-journal.md](./design/transaction-journal.md)). No upstream V4A tool provides `PREPARED` journals; that seam is original.

grep-app found **no** `*** Hash:` patch pins in the wild — BLAKE3 pins are an agent-patch extension (contract-v2).

---

## Seam: verify runner (argv, timeout, process group)

No V4A apply tool implements `--verify`. Closest production patterns for **bounded subprocess lifecycle**:

```rust
// std::os::unix::process::CommandExt — new process group
Command::new("prog")
    .arg("arg")
    .process_group(0)
    .spawn()?;
// then killpg(SIGTERM) / SIGKILL after grace

// tokio — prevent timeout orphans when using async Command
cmd.kill_on_drop(true);
let result = tokio::time::timeout(limit, cmd.output()).await;
```

Observed in: `rust-lang/rust` docs for `process_group(0)`; `archestra-ai/archestra` SIGTERM-then-SIGKILL on the group; many tokio tools using `kill_on_drop(true)` on timeout.

**agent-patch verify contract (frozen):**

- Canonical: `--verify -- <PROGRAM> [ARG …]` (no shell)
- Escape: `--verify-shell <SCRIPT>` only
- `cwd` = shadow root; env `AGENT_PATCH_*`
- Defaults: 10 min timeout, 5 s kill grace, 8 MiB/stream
- Prefer sync `std::process` + `process_group(0)` + group signals (CLI is sync today); if async later, also `kill_on_drop(true)`

Hard links forbidden in shadows (verifier must not mutate real inodes).

---

## Seam: protocol grammar (Move / EOF / anchors)

Agents tool description (Lark) matches Codex:

```text
UpdateFile := "*** Update File: " path NEWLINE [ MoveTo ] { Hunk }
MoveTo := "*** Move to: " newPath NEWLINE
Hunk := "@@" [ header ] NEWLINE { HunkLine } [ "*** End of File" NEWLINE ]
```

Move remains deferred ([design/move.md](./design/move.md)). EOF and `@@` anchors are v1.

---

## Seam mapping → plan modules

| Plan module | Ground-truth primary | Avoid |
| --- | --- | --- |
| `engine/locate` + `emit` | Agents `Chunk` / `_apply_chunks` | flickzeug apply |
| `engine/matcher` + `--fuzzy` | Codex/Agents ladder levels; **unique** gate is ours | first-match-wins |
| `MatchEvidence` / risk | Derived from our locator attempts | flickzeug similarity |
| Idempotence | After-state proof; flickzeug outcome enum as **UX analogy only** | reverse unified round-trip on V4A |
| Journal / objects / recover | Original; contrast Codex `AppliedPatchDelta` | assuming Codex atomicity |
| Verify runner | `process_group(0)` + kill grace; tokio `kill_on_drop` patterns | shell-by-default |
| Hash pins | Original grammar | inventing upstream precedent |
| Receipts / CAS | Original self-contained undo | hashes-only “receipts” |

## Tool notes

| Tool | Rule |
| --- | --- |
| opensrc | Prefer `owner/repo#main`; `openai/agents` is not a GitHub repo |
| grep-app | Literal code patterns; always pass MCP `server` + `toolName` |
