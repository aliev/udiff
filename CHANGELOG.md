# Changelog

All notable changes to μdiff are documented in this file. The project uses
[Semantic Versioning](https://semver.org/).

## [0.1.3] - 2026-09-07

### Added

- `w` stops long lines from folding and cuts them at the right edge instead.
  The view then follows the cursor sideways, so walking off the edge scrolls
  rather than stopping, and vim's `»` and `«` mark the parts hanging off each
  side. Hunk headers stay put: they label the code rather than being code, and
  are shorter than it.
- `^` and `$` move to the first non-blank character and to the end of the
  line.
- `b` hides and shows the file explorer. A `▸` in the file bar marks where it
  folded away, and `-` or `Tab` brings it back. The state lasts for the session
  only — μdiff keeps no settings.

## [0.1.2] - 2026-09-06

### Added

- `udiff --version` (and `-V`) reports the version, which a released binary
  previously could not be asked for at all.

### Fixed

- The keyboard help named `Shift+Enter` for inserting a newline, which the
  terminal cannot deliver unless the application negotiates an extended
  keyboard protocol, which μdiff does not. It now names `Ctrl+J`, the key that
  has always worked.

- The mouse no longer reaches the diff while the review editor or the file
  filter has focus. Scrolling used to walk the diff cursor and, with a review
  range open, keep extending the selection underneath the editor.

## [0.1.1] - 2026-09-06

### Added

- Readline motions in the comment and suggestion editor: `Ctrl+A` / `Ctrl+E`
  move to the start and end of the line, `Ctrl+K` / `Ctrl+U` kill to the end
  and to the start. `Home` and `End` follow the same rule.
- `e` opens `$EDITOR` at the line under the cursor for editors that can be told
  one — Vim, Neovim, Nano, Emacs, Kakoune, Helix, Sublime, Zed, VS Code and its
  forks, and the JetBrains family. Any other editor still receives only the
  path.

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

[0.1.3]: https://github.com/aliev/udiff/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/aliev/udiff/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/aliev/udiff/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/aliev/udiff/releases/tag/v0.1.0
