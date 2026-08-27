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
the README when changing the CLI, key bindings, or review Markdown contract.

## Review format compatibility

The Markdown format is a public interface. Backward-incompatible changes must
increment `format_version` and add an explicit parser for the new version.
Never silently reinterpret an existing version.
