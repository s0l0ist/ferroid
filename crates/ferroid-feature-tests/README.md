# Feature tests

I added this crate to exhaustively test Ferroid's feature combinations. Default and all-features builds
can miss unused code and tests that fail under specific features. This harness
requires warning-free compilation and passing tests.

## Usage

Run from the repository root. Requires `cargo-hack`.

```sh
# Test feature combinations
cargo run --release -p ferroid-feature-tests -- run

# Continue an interrupted run
cargo run --release -p ferroid-feature-tests -- resume

# Retry failures after fixing the code
cargo run --release -p ferroid-feature-tests -- retry-failed

# Show results
cargo run --release -p ferroid-feature-tests -- status

# Remove saved results and logs, keeping build caches
cargo run --release -p ferroid-feature-tests -- clean

# See options
cargo run --release -p ferroid-feature-tests -- run --help
```

Compiler warnings count as failures. Tests use `--lib` by default; run doctests
separately. Use `run --mode check` to check compilation without running tests.

Results, logs, and worker build caches live in this crate's `state/` directory.
Resume and retry keep passing results. Keep the same `--jobs` value to reuse the
same cache for each case. Use `--state PATH` for a separate matrix, or
`run --overwrite` to replace an existing one.
