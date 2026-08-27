# loopdiff

A fast Rust/Ratatui terminal UI for viewing unified diffs and patch files.

Loopdiff reads a diff from standard input, presents it in a quiet GitHub-like
interface, and lets you attach temporary comments to lines or ranges. Comments
can be copied as compact plain text for use in another tool.

## Install

### Homebrew

```bash
brew install aliev/tap/loopdiff
```

### Installer

macOS and Linux:

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/aliev/loopdiff/releases/latest/download/loopdiff-installer.sh | sh
```

Windows PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/aliev/loopdiff/releases/latest/download/loopdiff-installer.ps1 | iex"
```

### Build from source

Loopdiff requires Rust 1.88 or newer:

```bash
cargo install --path . --locked
```

## Usage

Loopdiff accepts a unified diff on standard input:

```bash
git diff | loopdiff
git diff master..HEAD | loopdiff
git diff --staged | loopdiff
cat changes.patch | loopdiff
```

Running `loopdiff` without piped input exits with a short usage hint. An empty
or unsupported input exits successfully with `loopdiff: nothing to view`.
Comments and viewed-file state are intentionally local to the current run.

### Watch mode

Open Loopdiff immediately and review quiet-period batches as files change:

```bash
loopdiff --watch .
```

Watch mode is in-memory and does not create a journal or require Git. Loopdiff
follows new batches while you are viewing the latest one. Use `{` and `}` to
move between older and newer batches. Cursor, comments, and viewed-file state
are retained independently for every batch. `Shift+Y` copies comments from
every batch in chronological order.

### Compare two files

The simplest way to review changes between two files is `diff -u`:

```bash
diff -u old.rs new.rs | loopdiff
```

The `-u` option produces the unified-diff format expected by Loopdiff. Git can
produce a similar diff without requiring the files to be inside a repository:

```bash
git diff --no-index --no-color -- old.rs new.rs | loopdiff
```

To compare directories, use either recursive `diff` or Git:

```bash
diff -ru old-dir/ new-dir/ | loopdiff
git diff --no-index --no-color -- old-dir/ new-dir/ | loopdiff
```

Both `diff` and `git diff --no-index` exit with status `1` when differences are
found. This is expected, although a shell configured with `pipefail` may report
the whole pipeline as unsuccessful.

Tools such as `delta` are useful for viewing diffs directly, but their output
contains terminal styling intended for humans. Feed the original uncolored
unified diff—not `delta` output—into Loopdiff.

## UI

- GitHub-style folder/file sidebar with fuzzy filtering.
- Separate old/new gutters and per-hunk syntax highlighting.
- Full-row add/remove backgrounds, cursor, and visual ranges.
- Inline multiline comments attached to lines or ranges.
- Comments appear as selectable children beneath their files.
- Mouse support plus Vim-style navigation.
- VS Code-style sticky hunk headers.

Clipboard yanking uses OSC 52 and therefore requires a terminal that permits
OSC 52 clipboard access.

## Keys

| Key | Action |
|---|---|
| `-`, `Tab` | toggle focus between sidebar and diff |
| `?` | open keyboard help |
| `j/k`, arrows | move |
| `G`, `gg`, `42gg` | end/start/jump to line |
| `c`, then `j/k` or arrows | select diff lines for a comment |
| `Enter`, double click | add or edit a comment |
| `Enter`, `Esc` | save/cancel the comment editor |
| `Shift+Enter` | insert a newline in a comment |
| `[` / `]` | previous/next comment |
| `d` / `u` | delete comment / undo deletion |
| `Space` | mark the current file viewed/unviewed and advance |
| `v`, then arrows or `h/j/k/l` | characterwise visual selection |
| `Shift+V`, then `j/k` or arrows | linewise visual selection |
| `y` | copy the visual selection |
| `Shift+Y` | copy all comments as compact plain text |
| `e` | open the current file in `$EDITOR` |
| `/` | filter files through the statusline |
| `h/l`, left/right | horizontally scroll the sidebar |
| `{` / `}` | previous/next Diffwatch batch |
| `Ctrl+U` / `Ctrl+D` | half-page up/down |
| `q` | quit |

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Loopdiff is available under the
[MIT License](LICENSE). Release notes are maintained in
[CHANGELOG.md](CHANGELOG.md), and the maintainer release process is documented
in [RELEASING.md](RELEASING.md).
