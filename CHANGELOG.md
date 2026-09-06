# Changelog

All notable changes to μdiff are documented in this file. The project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Readline motions in the comment and suggestion editor: `Ctrl+A` / `Ctrl+E`
  move to the start and end of the line, `Ctrl+K` / `Ctrl+U` kill to the end
  and to the start. `Home` and `End` follow the same rule.

## [0.1.0] - 2026-09-06

### Added

- μdiff, a universal unified-diff viewer that reads a diff from standard input
  and presents it in a quiet GitHub-inspired terminal interface.
- Syntax highlighting, file filtering, and Vim-style navigation with comment
  ranges, visual selection, and OSC 52 yanking.
- Temporary line and range comments that can be copied together as compact
  plain text.
- Code suggestions for contiguous new-side diff ranges, with syntax-highlighted
  inline editing, rendering, and clipboard export.
- Responsive layout that scales the sidebar with the terminal and shows a
  single pane below 64 columns, a review meter, a diff scrollbar, soft-wrap
  markers, and a scrollable keyboard help.
- Dark, light, and monochrome appearances, selected from `NO_COLOR`,
  `UDIFF_THEME`, and `COLORFGBG`.
- Watch mode behind the default `watch` feature, backed by `uwatch`.
- `uwatch`, a standalone filesystem watcher that groups changes into
  quiet-period batches and emits unified diffs.
- Linux, macOS, and Windows CI builds.

[Unreleased]: https://github.com/aliev/udiff/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/aliev/udiff/releases/tag/v0.1.0
