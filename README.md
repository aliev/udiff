# diffwatch workspace

This workspace contains two complementary terminal tools:

- [`diffwatch`](diffwatch/README.md) watches a directory and emits filesystem
  changes grouped into quiet-period batches.
- [`loopdiff`](loopdiff/README.md) renders unified diffs in an interactive TUI.

Build and test both projects from the workspace root:

```sh
cargo build --workspace
cargo test --workspace
```

Run an individual command with `cargo run -p diffwatch` or
`cargo run -p loopdiff`.

Loopdiff depends on Diffwatch only through its default `watch` feature. Its
stdin-only mode can be built independently of Diffwatch:

```sh
cargo build -p loopdiff --no-default-features
```

For live interactive review, build the workspace and let Loopdiff watch the
project directly:

```sh
cargo build --workspace
target/debug/loopdiff --watch .
```

With an installed binary:

```sh
loopdiff --watch .
```
