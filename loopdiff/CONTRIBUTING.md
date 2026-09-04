# Contributing

Thanks for helping improve Loopdiff.

## Development

Loopdiff requires Rust 1.85 or newer and Git. Clone the repository, then run:

```bash
cargo build
cargo run -- HEAD..main
```

Before opening a pull request, run the same checks as CI:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

Keep changes focused and add a regression test for user-visible behavior. Update
the README when changing the CLI, key bindings, or clipboard review contract.

## Clipboard review compatibility

The compact plain-text format copied by `Shift+Y` is a public interface. Keep
comments and suggestions unambiguous and cover format changes with exact-output
tests.
