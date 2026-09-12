# Changelog

All notable changes to μdiff are documented in this file. The project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- μdiff asks the terminal what colour its background actually is, with OSC 11,
  and follows the answer. `COLORFGBG` is a guess left in the environment —
  sometimes by a different terminal, usually before the last theme change — and
  most terminals never set it at all, so a light terminal used to need
  `UDIFF_THEME=light` to be told twice. An explicit `NO_COLOR` or `UDIFF_THEME`
  still outranks the answer, a terminal that will not answer within a tenth of
  a second is left alone, and inside tmux or screen the question is not asked
  at all: it needs their passthrough, and without it the reply can come back as
  a keystroke.

## [0.4.0] - 2026-09-09

### Added

- Comments and suggestions are marked in the gutter, in a column beside the
  change map and drawn the same way: an unbroken bar, lit where a note falls.
  A band covers many lines, so four notes on four adjacent lines share one
  cell — a glyph apiece invited counting them, and the count was always
  wrong. The column appears only for a file that has notes: one that says
  nothing is not worth its width in a split terminal.

### Changed

- The change map is one unbroken column beside the scrollbar. Every cell fills
  its width, dim where nothing changed and coloured where something did; a
  band holding both kinds splits top and bottom, removals above and additions
  below. A cell is a squashed slice of the file, so it divides the way the
  file runs — dividing it left and right instead turned a run of mixed bands
  into two parallel bars, and marking only the changed cells left a column
  jogging between a centred rule and a half block.
- The gutter paints in its own softer trio rather than the colours of a `-`,
  a `+` or a note's rule. A solid cell of a marker colour read far louder than
  the pale rows those markers sit on, so the gutter looked like a different
  palette, worst in the light theme. The new trio is the same hues pulled
  three quarters of the way towards the rows, and still clears three to one
  against the pane.

## [0.3.0] - 2026-09-09

### Added

- `p` finds a file by name: a list over the diff that narrows as you type, with
  each file's size and whether it has been reviewed. `Enter` opens it, `Esc`
  leaves the diff where it was. It reopens on the search it was left on, since
  coming back to the list usually means going somewhere near the last place;
  `Ctrl+U` starts over. The list is ordered the way the work goes — what is
  left before what is done, and within each, what has been written about
  before what has not — under headings that appear once the two groups exist.

- A change map beside the scrollbar: one column marking where the additions
  and removals are, so a long file shows at a glance how much is left and
  where it clusters. A cell stands for several lines and so usually holds both
  kinds; a half-block splits it, painting removals red on the left and
  additions green on the right, rather than spending a third colour to say
  "both". `n` and `N` jump between change blocks, counting a removal and the
  addition that replaced it as one stop. The map says where the changes are
  and the scrollbar says where the view is; reaching a change quickly needs
  both, so neither replaces the other.

### Changed

- The file explorer starts hidden. A panel costs a column for as long as it is
  up, which in a split terminal is most of the reading width, and `p` reaches a
  file without one. `b`, `-` and `Tab` still bring the explorer back.

### Fixed

- Scrolling to the end of a file no longer runs past the last line, which left
  blank rows under it and stopped the scrollbar's thumb short of the bottom of
  its own track. The margin kept below the cursor is context to read into, and
  past the last line there is none.
- A comment or suggestion on the last line of a file is no longer left without
  room to be drawn, and one taller than the margin below the cursor is no
  longer cut off. A card hangs under the line it belongs to, so it is what the
  margin was reserving room for; the two are no longer reserved separately.

### Removed

- `/`, which filtered the explorer from the status line. It showed the query
  but never the matches, so accepting it was a guess; `p` shows what it will
  open.

## [0.2.0] - 2026-09-07

### Added

- `s` shows the diff side by side, with each removal level with the addition
  that replaced it. Wrapping turns off while it is on, because aligned rows
  cost one screen row each.

### Changed

- `r` on a removed line moves to the addition that replaced it and offers the
  suggestion there, rather than refusing. A suggestion rewrites what is on
  disk, and what is on disk is the new side. A range you selected yourself is
  left alone, and commenting still stays on the line it is given.

### Fixed

- Watch mode dropped how you were reading the diff — wrapping, side by side,
  and the horizontal offset — every time a batch of edits landed, and moving
  between revisions restored whatever mode each one had been left in.
- `^` scrolled a line's indentation off the screen, hiding where the line
  began at the moment of asking to go there.

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

[Unreleased]: https://github.com/aliev/udiff/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/aliev/udiff/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/aliev/udiff/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/aliev/udiff/compare/v0.1.3...v0.2.0
[0.1.3]: https://github.com/aliev/udiff/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/aliev/udiff/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/aliev/udiff/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/aliev/udiff/releases/tag/v0.1.0
