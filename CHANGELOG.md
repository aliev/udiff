# Changelog

All notable changes to μdiff are documented in this file. The project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Code suggestions for contiguous new-side diff ranges, with syntax-highlighted
  inline editing, rendering, and clipboard export.

### Changed

- Renamed the filesystem watcher package and executable to `uwatch`.
- Renamed the application to μdiff and its executable to `udiff`.
- μdiff is now a universal unified-diff viewer that reads standard input.
- Comments are temporary and can be copied together as compact plain text.
- Removed Git input modes, persistent Markdown sessions, validation, replies,
  roles, authors, and external-response synchronization.

### Fixed

- Keep the active diff cursor visible when long inline suggestions appear
  earlier in the viewport.
- Avoid a redundant terminal clear that could block before the first frame
  when the diff was supplied through a pipe.
- Use Crossterm's controlling-TTY event source so keyboard input remains
  available after a diff is consumed from standard input.

## [0.1.0] - 2026-08-08

### Added

- Git revision, staged, working-tree, file-to-file, and stdin diff modes.
- GitHub-inspired terminal diff with syntax highlighting and file filtering.
- Inline human/AI review threads with named authors and multiline comments.
- Versioned, self-describing Markdown handoff format with strict validation.
- Safe external-response watcher and conflict-aware session persistence.
- Vim-style navigation, comment ranges, visual selection, and OSC 52 yanking.
- Linux, macOS, and Windows CI builds.
