# Fuzz targets

Requires nightly + `cargo-fuzz`:

```bash
cargo install cargo-fuzz
cargo +nightly fuzz run parse_patch -- -max_total_time=60
cargo +nightly fuzz run path_policy -- -max_total_time=60
cargo +nightly fuzz run apply_update -- -max_total_time=60
```

Invariants: no panic; parse/path/apply stay within existing fail-closed errors.
