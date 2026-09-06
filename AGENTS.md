# AGENTS.md

This file provides guidance for coding agents working in this repository.

## Project Overview

μdiff is a small Rust terminal UI for viewing unified diffs received on
standard input. Its `--watch` mode uses the Uwatch library to observe a
directory in memory and keeps independent review state for each completed
batch. It presents a GitHub-inspired diff, lets the user attach temporary
comments or code suggestions to lines or ranges, and copies them as compact
plain text through OSC 52.

Keep the application fast, keyboard-friendly, visually quiet, and independent
of Git repositories or review persistence formats.

## Repository Structure

- `Cargo.toml`: virtual workspace manifest that builds both packages.
- `udiff/Cargo.toml`: μdiff package manifest and optional Uwatch dependency.
- `uwatch/`: independently useful secondary package providing filesystem
  observation and the public watcher API used by the optional `watch` feature.
- `udiff/src/main.rs`: composition root and exit behavior.
- `udiff/src/input.rs`: the `DiffSource` boundary and stdin implementation.
- `udiff/src/terminal.rs`: terminal lifecycle, event loop, and external effects.
- `udiff/src/model.rs`: unified-diff parsing, file/line models, syntax highlighting.
- `udiff/src/highlight.rs`: the Syntect implementation.
- `udiff/src/comment.rs`: the in-memory comment and code-suggestion model.
- `udiff/src/app.rs`: the UI composition root; it only declares the assembled
  parts.
- `udiff/src/app/command.rs`: the `Command`/`Effect` boundary.
- `udiff/src/app/session.rs`: diff, comments, reviewed state, and comment
  history.
- `udiff/src/app/diff_pane.rs`: diff viewport state and navigation semantics.
- `udiff/src/app/diff_view.rs`: diff rendering.
- `udiff/src/app/file_tree.rs`: explorer state, navigation, filtering, and
  rendering.
- `udiff/src/app/comment_editor.rs`: UTF-8-safe editor state and actions.
- `udiff/src/app/navigation.rs`: file selection, sidebar focus, and search flow.
- `udiff/src/app/review.rs`: comment, suggestion, export, and reviewed-file
  workflows.
- `udiff/src/app/revisions.rs`: independent state for watched revisions.
- `udiff/src/app/view.rs`: top-level screen layout.
- `udiff/src/app/statusline.rs` and `help.rs`: focused UI components.
- `udiff/src/app/controller.rs`: concise keyboard/mouse routing and command
  boundary.
- `udiff/src/app/view_helpers.rs`: terminal-width-aware rendering primitives.
- `udiff/src/app/tests.rs`: cross-component UI characterization tests.
- `ARCHITECTURE.md`: reading order and ownership map.
- `README.md`: installation, usage, and key bindings.

## Development Commands

Run these from the repository root:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --release
```

The minimum supported Rust version is 1.88.

Use `cargo fmt` after editing Rust files. Add focused regression tests for
behavioral or rendering fixes.

Useful manual runs:

```bash
git diff | cargo run -p udiff --release
git diff master..HEAD | cargo run -p udiff --release
cat changes.patch | cargo run -p udiff --release
```

## Architecture

- Diff semantics belong in `udiff/src/model.rs`.
- Comment and suggestion data belongs in `udiff/src/comment.rs`.
- Review workflows belong in `udiff/src/app/review.rs`; navigation belongs in
  `udiff/src/app/navigation.rs`.
- Rendering state belongs to its concrete component under `app/`.
- Input adapters belong in `udiff/src/input.rs`; terminal lifecycle belongs in
  `udiff/src/terminal.rs`.
- Keep the integration one-way: Uwatch must not depend on μdiff, and μdiff must
  use Uwatch's public watcher API rather than its journal layout.

Do not add Git subprocess behavior, repository discovery, persistent review
formats, agent protocols, roles, replies, or authors. Review items exist only
for the current process and `Shift+Y` exports them as plain text for the
clipboard.
Keep the Uwatch watcher adapter in `udiff/src/input.rs` and its review-state
transitions in `udiff/src/app.rs`.

## UX Invariants

- `-` switches focus between the file tree and diff; `Tab` remains an alias.
- `?` opens modal keyboard help.
- The sidebar shows comments as selectable children of their file.
- `/` opens search in the shared statusline and filters the sidebar.
- `gg` jumps to the start and `{number}gg` jumps to a visible old/new line.
- `c` starts or clears the diff-line range used for review.
- `r` suggests a replacement for the current line or selected new-side range.
- `Space` toggles the current file's in-memory reviewed state.
- `v` and `Shift+V` start characterwise and linewise visual selection.
- `y` copies visual selection without diff markers.
- `Shift+Y` copies all comments and suggestions as compact plain text without
  metadata or instructions.
- New range comments render below the visually lowest selected line.
- Enter edits an exact anchored comment and creates a comment elsewhere.
- In the editor, Enter saves, Shift+Enter inserts a newline, and Esc cancels.
- Handle terminal line feed (`Ctrl+J`) directly for Shift+Enter.
- Editor movement and editing must remain safe at UTF-8 boundaries.
- Add/remove backgrounds extend to the right edge of the viewport.
- Rendering must tolerate narrow and short terminals without panicking.

## Rendering and Safety

- Use Ratatui primitives; do not emit terminal escape codes from widgets.
- Account for Unicode display width when cropping or padding.
- Keep syntax foreground colors independent from diff backgrounds.
- Restore raw mode, alternate screen, mouse capture, and cursor visibility on
  every normal error path.
- Modified key events must not accidentally insert characters into the editor.

## Change Discipline

- Keep changes scoped and avoid unrelated dependencies.
- Never discard user changes in a dirty worktree.
- Update `README.md` for user-visible behavior or key changes.
- Do not commit build output or `target/`.
- For bug fixes, add a regression test that fails before the fix.
