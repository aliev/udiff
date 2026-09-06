# μdiff

A focused Rust/Ratatui terminal UI for reviewing unified diffs and patch files.

μdiff reads a diff from standard input, presents it in a quiet GitHub-like
interface, and lets you attach temporary comments and code suggestions to lines
or ranges. Review items can be copied as compact plain text for use in another
tool. Its executable is named `udiff` so it stays quick to type.

## Install

### Homebrew

```bash
brew install aliev/tap/udiff
```

### Installer

macOS and Linux:

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/aliev/udiff/releases/latest/download/udiff-installer.sh | sh
```

Windows PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/aliev/udiff/releases/latest/download/udiff-installer.ps1 | iex"
```

### Build from source

μdiff requires Rust 1.88 or newer:

```bash
cargo install --path . --locked
```

The default `watch` feature adds the optional Diffwatch integration. To build
μdiff as a standalone stdin diff viewer without the Diffwatch dependency:

```bash
cargo install --path . --locked --no-default-features
```

In that build, passing `--watch` reports that the feature is unavailable.

## Usage

μdiff accepts a unified diff on standard input:

```bash
git diff | udiff
git diff master..HEAD | udiff
git diff --staged | udiff
cat changes.patch | udiff
```

Running `udiff` without piped input exits with a short usage hint. An empty or
unsupported input exits successfully with `udiff: nothing to view`.
Comments, suggestions, and reviewed-file state are intentionally local to the
current run.

### Watch mode

Watch mode is provided by the default `watch` Cargo feature.

Open μdiff immediately and review revisions as files change:

```bash
udiff --watch .
```

Pipe an existing diff to seed the review with a context revision before live
filesystem revisions arrive:

```bash
git diff | udiff --watch .
```

Watch mode is in-memory and does not create a journal or require Git. Each
quiet-period batch from Diffwatch appears as a revision in μdiff. μdiff
follows new revisions while you are viewing the latest one. Use `{` and `}` to
move between older and newer revisions. Cursor, review items, and reviewed-file
state are retained independently for every revision. `Shift+Y` copies comments
and suggestions from every revision in chronological order. When stdin provides
the initial diff, it appears as `revision 1/N · context`; the watcher still
snapshots the current directory normally and numbers subsequent live revisions
from `#1`.

### Compare two files

The simplest way to review changes between two files is `diff -u`:

```bash
diff -u old.rs new.rs | udiff
```

The `-u` option produces the unified-diff format expected by μdiff. Git can
produce a similar diff without requiring the files to be inside a repository:

```bash
git diff --no-index --no-color -- old.rs new.rs | udiff
```

To compare directories, use either recursive `diff` or Git:

```bash
diff -ru old-dir/ new-dir/ | udiff
git diff --no-index --no-color -- old-dir/ new-dir/ | udiff
```

Both `diff` and `git diff --no-index` exit with status `1` when differences are
found. This is expected, although a shell configured with `pipefail` may report
the whole pipeline as unsuccessful.

Tools such as `delta` are useful for viewing diffs directly, but their output
contains terminal styling intended for humans. Feed the original uncolored
unified diff—not `delta` output—into μdiff.

## UI

- GitHub-style folder/file sidebar with fuzzy filtering.
- Separate old/new gutters and per-hunk syntax highlighting.
- Full-row add/remove backgrounds, cursor, and visual ranges.
- Inline multiline comments attached to lines or ranges.
- Syntax-highlighted code suggestions that replace contiguous new-side or
  context lines.
- Comments appear as selectable children beneath their files.
- Mouse support plus Vim-style navigation.
- VS Code-style sticky hunk headers.

Clipboard yanking uses OSC 52 and therefore requires a terminal that permits
OSC 52 clipboard access.

## Workspace

μdiff is the root package. [`diffwatch/`](diffwatch/README.md) is an
independently useful secondary package that provides the optional in-memory
watcher integration. Validate both packages with `cargo test --workspace`.

## Keys

| Key | Action |
|---|---|
| `-`, `Tab` | toggle focus between sidebar and diff |
| `?` | open keyboard help |
| `j/k`, arrows | move |
| `G`, `gg`, `42gg` | end/start/jump to line |
| `c`, then `j/k` or arrows | select diff lines for review |
| `Enter`, double click | add or edit a comment |
| `r` | suggest a replacement for the current line or selected range |
| `Enter`, `Esc` | save/cancel the review editor |
| `Shift+Enter` | insert a newline in a comment or suggestion |
| `[` / `]` | previous/next comment |
| `d` / `u` | delete comment / undo deletion |
| `Space` | mark the current file reviewed/reopen it and advance |
| `v`, then arrows or `h/j/k/l` | characterwise visual selection |
| `Shift+V`, then `j/k` or arrows | linewise visual selection |
| `y` | copy the visual selection |
| `Shift+Y` | copy comments and suggestions from files not marked reviewed |
| `Shift+R` | clear in-memory revisions and return to the waiting screen |
| `e` | open the current file in `$EDITOR` |
| `/` | filter files through the statusline |
| `h/l`, left/right | close/open a tree node in the sidebar |
| `{` / `}` | previous/next revision |
| `Ctrl+U` / `Ctrl+D` | half-page up/down |
| `q` | quit |

## Contributing

Start with [ARCHITECTURE.md](ARCHITECTURE.md) to understand the codebase, then
see [CONTRIBUTING.md](CONTRIBUTING.md). μdiff is available under the
[MIT License](LICENSE). Release notes are maintained in
[CHANGELOG.md](CHANGELOG.md), and the maintainer release process is documented
in [RELEASING.md](RELEASING.md).
