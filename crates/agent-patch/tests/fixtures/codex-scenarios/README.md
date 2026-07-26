# Codex apply-patch scenario subset

Portable fixtures from `openai/codex` `codex-rs/apply-patch/tests/fixtures/scenarios/`.

Included only when compatible with our contract:

- unique exact match (no whitespace/unicode fuzz)
- Add never overwrites
- no `*** Move to:`
- no pure-addition Update append on non-empty files (Codex `016_*`)
- transactional apply (Codex `015_*` partial-leave excluded)

Excluded on purpose: `004_*`, `010_*` (Move), `011_*` (overwrite Add), `015_*`, `016_*`, `017_*`/`018_*`/`020_whitespace_*` (padding/fuzz).

`expect_failure` scenarios keep `expected/` identical to `input/` (tree unchanged).
