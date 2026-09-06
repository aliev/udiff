# Contributing

Thanks for helping improve μdiff.

## Development

μdiff requires Rust 1.88 or newer. Clone the repository, then run:

```bash
cargo build --workspace
git diff | cargo run -p udiff --release
```

Before opening a pull request, run the same checks as CI:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --release
```

Keep changes focused and add a regression test for user-visible behavior. Update
the README when changing the CLI, key bindings, or clipboard review contract.

## Clipboard review compatibility

The compact plain-text format copied by `Shift+Y` is a public interface. Keep
comments and suggestions unambiguous and cover format changes with exact-output
tests.
