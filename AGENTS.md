# Workspace guidance

This Cargo workspace contains two independently useful binaries. Loopdiff uses
the Diffwatch library directly for its in-memory watch mode:

- `diffwatch/` owns filesystem observation, snapshots, quiet-period batching,
  optional journal persistence, and its in-memory watcher API.
- `loopdiff/` owns unified-diff parsing, interactive review state, rendering,
  and live watcher presentation.

Keep the integration one-way: Diffwatch must not depend on Loopdiff, and
Loopdiff should use the public watcher API rather than Diffwatch's on-disk
journal layout.

Run workspace checks from this directory:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The minimum supported Rust version is 1.88. Add focused regression tests for
protocol changes, batching behavior, and interactive state transitions.
