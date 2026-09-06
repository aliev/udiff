# Loopdiff architecture

Loopdiff is intentionally a small application, not a framework. Its design has
three boundaries:

```text
stdin / Diffwatch (`watch` feature)
       │
       ▼
 unified diff ──► model ──► App::update ──► component state
                                  │
                                  ▼
                               Effect
                                  │
                                  ▼
                         terminal / clipboard / editor
```

`App::update` is the only entry point for input. It updates in-memory state and
returns an `Effect` when the outside world must do something. `terminal.rs`
performs those effects. This keeps parsing, review behavior, and rendering easy
to test without a real terminal.

## Read the code in this order

1. `main.rs` chooses stdin or watch mode.
2. `input.rs` turns input into unified-diff text or watch events.
3. `model.rs` parses that text into files and lines.
4. `comment.rs` defines comments, suggestions, and clipboard output.
5. `app.rs` shows all application state in one place.
6. `app/controller.rs` routes keys and mouse events.
7. Follow one route into `review.rs`, `navigation.rs`, or `revisions.rs`.
8. Read `diff_pane.rs` for cursor/selection behavior and `diff_view.rs` for its
   rendering.
9. `terminal.rs` owns raw mode, the event loop, clipboard access, and `$EDITOR`.

You do not need to understand rendering before changing review behavior.

## Where a change belongs

| Change | Start here |
|---|---|
| Unified-diff parsing or line numbers | `model.rs` |
| Syntax highlighting | `highlight.rs` |
| Comment/suggestion data or copied prompt format | `comment.rs` |
| Create, edit, delete, or export review items | `app/review.rs` |
| Cursor, range, visual selection, or yank | `app/diff_pane.rs` |
| File tree, focus, or search | `app/navigation.rs`, `app/file_tree.rs` |
| Watch revision history | `app/revisions.rs` |
| Key or mouse routing | `app/controller.rs` |
| Screen layout | `app/view.rs` |
| Diff appearance | `app/diff_view.rs`, `app/render.rs` |
| OS/terminal/clipboard/editor behavior | `terminal.rs` |

## State ownership

- `Session` owns the current diff, review items, reviewed files, and undo data.
- `DiffPane` owns only navigation and selection state for the diff.
- `FileTree`, `CommentEditor`, `Help`, and `Statusline` own their widget state.
- `App` coordinates those concrete components directly.
- `RevisionState` stores the same review state for inactive watch revisions.

There is no service container, generic component trait, event bus, or persistent
review model. Add one only if a concrete feature cannot stay simple without it.

## Safe feature workflow

For a new behavior, add one focused test at its owner, implement it there, and
only then add the key route or external effect. Cross-component behavior belongs
in `app/tests.rs`; local parsing and state rules belong next to their module.

Run from the workspace root:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
