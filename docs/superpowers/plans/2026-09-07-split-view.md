# Split View Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show a diff side by side — what was there on the left, what replaced it on the right, aligned — behind a session-only toggle.

**Architecture:** A pure `rows` module pairs a line list into rows. `DiffPane` keeps its `cursor` as an index into lines and gains only which side that line sits on, so no review behaviour changes. Navigation and rendering are the only readers of the pairing.

**Tech Stack:** Rust 1.88, ratatui 0.30, crossterm 0.29. No new dependencies.

**Spec:** `docs/superpowers/specs/2026-09-07-split-view-design.md`

## Global Constraints

- No new crate dependencies.
- After every task, from the workspace root: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`. All three must pass before committing.
- Clippy runs with `-D warnings`, so no task may leave unread state or unused functions behind. Every task ends with the tree compiling warning-free.
- Split and soft wrapping exclude each other: turning on one turns off the other.
- `DiffPane::cursor` keeps its meaning — an index into the current file's `lines`. Nothing in this plan changes that.
- The mode lasts for the session. μdiff has no settings file.
- Conventional commit messages. Do not push.

---

### Task 1: Pairing, state, and navigation

Rows and the cursor's side, with navigation that uses them. Rendering is still
unified after this task, so the mode is invisible on screen and proven by tests
alone. Task 2 draws it.

**Files:**
- Create: `udiff/src/app/rows.rs`
- Modify: `udiff/src/app.rs` (add `mod rows;`)
- Modify: `udiff/src/app/diff_pane.rs` (state, `s`, navigation)
- Test: `udiff/src/app/rows.rs` (inline `mod tests`), `udiff/src/app/tests.rs`

**Interfaces:**
- Consumes: `crate::model::{DiffLine, LineKind}`.
- Produces:
  - `crate::app::rows::Side` — `enum Side { Left, Right }`, `Clone + Copy + Debug + Eq + PartialEq`
  - `crate::app::rows::Row` — `struct Row { pub left: Option<usize>, pub right: Option<usize>, pub full_width: bool }`, same derives, with `fn occupant(&self, side: Side) -> Option<usize>` and `fn holds(&self, line: usize) -> bool`
  - `crate::app::rows::pair(lines: &[DiffLine]) -> Vec<Row>`
  - `DiffPane` fields `split: bool`, `side: Side`, `rows: Vec<Row>`

- [ ] **Step 1: Write the failing pairing tests**

Create `udiff/src/app/rows.rs` containing only this test module. It will not
compile until Step 3 adds the items it names.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::parse_unified_diff;

    fn rows_of(diff: &str) -> Vec<Row> {
        pair(&parse_unified_diff(diff)[0].lines)
    }

    #[test]
    fn a_replaced_line_shares_its_row_with_what_replaced_it() {
        let rows = rows_of(
            "--- a\n+++ b\n@@ -1,2 +1,2 @@\n ctx\n-gone\n+kept\n",
        );
        // [0] hunk header, [1] context, [2] the pair.
        assert!(rows[0].full_width);
        assert_eq!((rows[1].left, rows[1].right), (Some(1), Some(1)));
        assert_eq!((rows[2].left, rows[2].right), (Some(2), Some(3)));
    }

    #[test]
    fn the_longer_run_leaves_empty_cells_opposite_its_tail() {
        let rows = rows_of("--- a\n+++ b\n@@ -1,1 +1,3 @@\n-one\n+one\n+two\n+three\n");
        let pairs = &rows[1..];
        assert_eq!((pairs[0].left, pairs[0].right), (Some(1), Some(2)));
        assert_eq!((pairs[1].left, pairs[1].right), (None, Some(3)));
        assert_eq!((pairs[2].left, pairs[2].right), (None, Some(4)));
    }

    #[test]
    fn additions_with_nothing_removed_take_the_right_alone() {
        let rows = rows_of("--- a\n+++ b\n@@ -0,0 +1,2 @@\n+one\n+two\n");
        assert_eq!((rows[1].left, rows[1].right), (None, Some(1)));
        assert_eq!((rows[2].left, rows[2].right), (None, Some(2)));
    }

    #[test]
    fn removals_with_nothing_added_take_the_left_alone() {
        let rows = rows_of("--- a\n+++ b\n@@ -1,2 +0,0 @@\n-one\n-two\n");
        assert_eq!((rows[1].left, rows[1].right), (Some(1), None));
        assert_eq!((rows[2].left, rows[2].right), (Some(2), None));
    }

    #[test]
    fn context_between_them_keeps_the_runs_apart() {
        // The additions do not follow the removals immediately, so they are
        // their own run rather than the other half of one.
        let rows = rows_of("--- a\n+++ b\n@@ -1,3 +1,3 @@\n-gone\n ctx\n+kept\n");
        assert_eq!((rows[1].left, rows[1].right), (Some(1), None));
        assert_eq!((rows[2].left, rows[2].right), (Some(2), Some(2)));
        assert_eq!((rows[3].left, rows[3].right), (None, Some(3)));
    }

    #[test]
    fn a_row_reports_who_is_in_it() {
        let row = Row {
            left: Some(4),
            right: None,
            full_width: false,
        };
        assert_eq!(row.occupant(Side::Left), Some(4));
        assert_eq!(row.occupant(Side::Right), None);
        assert!(row.holds(4));
        assert!(!row.holds(5));
    }
}
```

Register the module in `udiff/src/app.rs`, keeping the `mod` lines in
alphabetical order — it goes between `revisions` and `search`:

```rust
mod rows;
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p udiff --bin udiff rows::`
Expected: FAIL to compile — `Row`, `Side`, and `pair` do not exist.

- [ ] **Step 3: Write the pairing**

Put this above the test module in `udiff/src/app/rows.rs`:

```rust
//! Pairs a unified diff into rows: what was there beside what replaced it.
//!
//! Only navigation and rendering read this. Review state addresses lines by
//! index and is unaffected by how they are laid out.

use crate::model::{DiffLine, LineKind};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Side {
    Left,
    Right,
}

impl Side {
    pub(super) fn other(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Row {
    pub left: Option<usize>,
    pub right: Option<usize>,
    /// Hunk headers and metadata label the code rather than being one side of
    /// it, so they run across both panes.
    pub full_width: bool,
}

impl Row {
    pub(super) fn occupant(&self, side: Side) -> Option<usize> {
        match side {
            Side::Left => self.left,
            Side::Right => self.right,
        }
    }

    pub(super) fn holds(&self, line: usize) -> bool {
        self.left == Some(line) || self.right == Some(line)
    }
}

pub(super) fn pair(lines: &[DiffLine]) -> Vec<Row> {
    let mut rows = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        match lines[index].kind {
            LineKind::Hunk | LineKind::Meta => {
                rows.push(Row {
                    left: Some(index),
                    right: None,
                    full_width: true,
                });
                index += 1;
            }
            LineKind::Context => {
                rows.push(Row {
                    left: Some(index),
                    right: Some(index),
                    full_width: false,
                });
                index += 1;
            }
            LineKind::Remove | LineKind::Add => {
                // Only additions that follow removals immediately are the
                // other half of the same change; anything between them makes
                // two runs that stand on their own.
                let removed = run(lines, index, LineKind::Remove);
                let added = run(lines, index + removed, LineKind::Add);
                for offset in 0..removed.max(added) {
                    rows.push(Row {
                        left: (offset < removed).then_some(index + offset),
                        right: (offset < added).then_some(index + removed + offset),
                        full_width: false,
                    });
                }
                index += removed + added;
            }
        }
    }
    rows
}

fn run(lines: &[DiffLine], start: usize, kind: LineKind) -> usize {
    lines[start.min(lines.len())..]
        .iter()
        .take_while(|line| line.kind == kind)
        .count()
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p udiff --bin udiff rows::`
Expected: PASS, 6 tests.

The build will warn that `Side::other`, `occupant`, and the new items are
unused outside tests. Step 5 gives them readers. Do not silence them with
`allow`.

- [ ] **Step 5: Give the pane the mode and the side**

In `udiff/src/app/diff_pane.rs`, extend the imports and the struct:

```rust
use super::rows::{Row, Side, pair};
```

```rust
pub struct DiffPane {
    // ... existing fields ...
    /// Side-by-side rather than unified. Session-only, like every other view
    /// preference here.
    pub split: bool,
    /// Which pane the cursor sits in. Not read while the view is unified.
    pub side: Side,
    /// Pairing for the current file. It does not depend on the mode, so it is
    /// rebuilt only when the file changes.
    pub rows: Vec<Row>,
}
```

In `DiffPane::new`, after the existing fields:

```rust
            split: false,
            side: Side::Left,
            rows: files.first().map(|file| pair(&file.lines)).unwrap_or_default(),
```

In `switch_file`, after `self.file = file;`:

```rust
        self.rows = pair(&files[file].lines);
```

- [ ] **Step 6: Write the failing navigation tests**

Add to `udiff/src/app/tests.rs`:

```rust
#[test]
fn walking_down_in_split_stays_on_one_side_until_it_runs_out() {
    // Two removals replaced by three additions: the left runs out first.
    let diff = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1,2 +1,3 @@\n-one\n-two\n+one\n+two\n+three\n";
    let mut app = App::new(parse_unified_diff(diff), Vec::new());
    app.key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
    assert_eq!(app.diff_pane.cursor, 1, "starts on the first removal");
    assert_eq!(app.diff_pane.side, Side::Left);

    app.key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    assert_eq!(app.diff_pane.cursor, 2, "the second removal, still on the left");
    assert_eq!(app.diff_pane.side, Side::Left);

    // The left has nothing on the next row, so the cursor crosses rather than
    // stopping.
    app.key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    assert_eq!(app.diff_pane.cursor, 5, "the third addition");
    assert_eq!(app.diff_pane.side, Side::Right);
}

#[test]
fn split_and_wrapping_turn_each_other_off() {
    let diff = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-old\n+new\n";
    let mut app = App::new(parse_unified_diff(diff), Vec::new());
    assert!(app.diff_pane.wrap, "wrapping is the default");

    app.key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
    assert!(app.diff_pane.split);
    assert!(!app.diff_pane.wrap, "aligned rows cost one screen row each");

    app.key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::NONE));
    assert!(app.diff_pane.wrap);
    assert!(!app.diff_pane.split);
}

#[test]
fn entering_split_lands_on_the_side_the_line_belongs_to() {
    let diff = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-old\n+new\n";
    let mut app = App::new(parse_unified_diff(diff), Vec::new());
    app.diff_pane.cursor = 2; // the addition
    app.key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
    assert_eq!(app.diff_pane.side, Side::Right);
    assert_eq!(app.diff_pane.cursor, 2, "and does not move the cursor");

    // Leaving changes nothing about which line is current.
    app.key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
    assert_eq!(app.diff_pane.cursor, 2);
}

#[test]
fn the_edge_of_a_line_steps_across_to_the_other_pane() {
    let diff = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-old\n+new\n";
    let mut app = App::new(parse_unified_diff(diff), Vec::new());
    app.key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
    app.key(KeyEvent::new(KeyCode::Char('$'), KeyModifiers::NONE));

    app.key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
    assert_eq!(app.diff_pane.side, Side::Right);
    assert_eq!(app.diff_pane.cursor, 2);

    app.key(KeyEvent::new(KeyCode::Char('^'), KeyModifiers::NONE));
    app.key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE));
    assert_eq!(app.diff_pane.side, Side::Left);
    assert_eq!(app.diff_pane.cursor, 1);
}
```

Add the import at the top of `udiff/src/app/tests.rs`:

```rust
use super::rows::Side;
```

- [ ] **Step 7: Run the tests to verify they fail**

Run: `cargo test -p udiff --bin udiff "walking_down_in_split\|split_and_wrapping\|entering_split\|the_edge_of_a_line"`
Expected: FAIL — `s` does nothing, so `split` stays false.

- [ ] **Step 8: Write the navigation**

In `udiff/src/app/diff_pane.rs`, add these methods to `impl DiffPane`:

```rust
    fn row_of(&self, line: usize) -> usize {
        self.rows
            .iter()
            .position(|row| row.holds(line))
            .unwrap_or(0)
    }

    /// The side a line belongs to, so entering split does not have to guess.
    fn side_of(&self, line: usize) -> Side {
        match self.rows.get(self.row_of(line)) {
            Some(row) if row.right == Some(line) && row.left != Some(line) => Side::Right,
            _ => Side::Left,
        }
    }

    fn step_row(&mut self, delta: isize, files: &[FileDiff]) {
        let current = self.row_of(self.cursor) as isize;
        let last = self.rows.len().saturating_sub(1) as isize;
        let Some(row) = self.rows.get(current.saturating_add(delta).clamp(0, last) as usize)
        else {
            return;
        };
        let (row, side) = (*row, self.side);
        if row.full_width {
            if let Some(line) = row.left {
                self.cursor = line;
            }
        } else if let Some(line) = row.occupant(side) {
            self.cursor = line;
        } else if let Some(line) = row.occupant(side.other()) {
            // This side has run out; the change continues on the other one.
            self.side = side.other();
            self.cursor = line;
        }
        let column_max = self.active_lines(files)[self.cursor]
            .text
            .chars()
            .count()
            .saturating_sub(1);
        self.visual_col = self.visual_col.min(column_max);
    }

    /// Crosses to the other pane when the cursor is already at the edge of its
    /// line, which is where a side-by-side reader expects to leave it.
    fn cross(&mut self, side: Side) -> bool {
        if !self.split || self.side == side {
            return false;
        }
        let row = self.rows[self.row_of(self.cursor)];
        let Some(line) = row.occupant(side) else {
            return false;
        };
        self.side = side;
        self.cursor = line;
        true
    }
```

Then route the keys. Replace the existing `j`/`k` arms so they step by row in
split mode, and give `h`/`l` the crossing behaviour at the edges:

```rust
            KeyCode::Char('s') if focused => {
                self.split = !self.split;
                if self.split {
                    // Aligned rows cost one screen row each, so a folded line
                    // would drift the two sides apart.
                    self.wrap = false;
                    self.h_scroll = 0;
                    self.side = self.side_of(self.cursor);
                }
            }
```

In the existing `w` arm, add `self.split = false;` alongside the wrap toggle.

In the `j` and `k` arms, replace `self.move_cursor(1, files)` and
`self.move_cursor(-1, files)` with:

```rust
                if self.split {
                    self.step_row(1, files);
                } else {
                    self.move_cursor(1, files);
                }
```

and the same with `-1` for `k`.

In the `h` arm, before moving the column:

```rust
                if self.visual_col == 0 && self.cross(Side::Left) {
                    return KeyAction::Consumed;
                }
```

In the `l` arm, before moving the column:

```rust
                let last = self.active_lines(files)[self.cursor]
                    .text
                    .chars()
                    .count()
                    .saturating_sub(1);
                if self.visual_col >= last && self.cross(Side::Right) {
                    return KeyAction::Consumed;
                }
```

- [ ] **Step 9: Run the whole suite**

Run:
```bash
cargo fmt --all
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
Expected: all pass. The mode is not drawn yet, so nothing on screen changes.

- [ ] **Step 10: Commit**

```bash
git add udiff/src/app/rows.rs udiff/src/app.rs udiff/src/app/diff_pane.rs udiff/src/app/tests.rs
git commit -m "feat(udiff): pair a diff into rows and let the cursor pick a side

A side-by-side view needs to know which addition replaced which removal, and
that is a property of the line list rather than of the screen, so it lives in
its own module and is tested on its own.

The cursor stays an index into the lines. It gains only the side it sits on,
which keeps every consumer of it — comments, ranges, yanking, anchoring —
working on the same terms as before. Walking down a run of removals continues
into the additions that replace it rather than stopping at the last one.

Split and wrapping turn each other off: aligned rows cost one screen row
each, and a line that folds further on one side than the other drifts the
comparison apart."
```

---

### Task 2: Drawing two panes

Each rendered row is **one** `Line` spanning the whole pane — left cell,
divider, right cell — rather than two independent columns. That is what lets a
hunk header and a comment card run across both sides, and it keeps `row_map`
one entry per drawn row exactly as the unified path does.

**Files:**
- Modify: `udiff/src/app/diff_view.rs`
- Test: `udiff/src/app/tests.rs`

**Interfaces:**
- Consumes: `crate::app::rows::{Row, Side}` and `DiffPane::{split, side, rows}` from Task 1.
- Produces: nothing later tasks depend on.

- [ ] **Step 1: Write the failing rendering tests**

Add to `udiff/src/app/tests.rs`:

```rust
fn screen(terminal: &Terminal<TestBackend>, width: u16, height: u16) -> Vec<String> {
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| terminal.backend().buffer().cell((x, y)).unwrap().symbol())
                .collect::<String>()
        })
        .collect()
}

#[test]
fn split_puts_a_removal_level_with_what_replaced_it() {
    let diff = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1,2 +1,2 @@\n ctx\n-old\n+new\n";
    let mut app = App::new(parse_unified_diff(diff), Vec::new());
    app.key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
    let mut terminal = Terminal::new(TestBackend::new(120, 12)).unwrap();
    terminal.draw(|frame| app.draw(frame)).unwrap();
    let rows = screen(&terminal, 120, 12);

    let paired = rows
        .iter()
        .find(|row| row.contains("old") && row.contains("new"))
        .expect("the removal and what replaced it share a row");
    assert!(
        paired.find("old") < paired.find('│') && paired.find('│') < paired.find("new"),
        "with the divider between them: {paired:?}"
    );
}

#[test]
fn a_hunk_header_and_a_comment_still_cross_both_panes() {
    let diff = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1,2 +1,2 @@\n ctx\n-old\n+new\n";
    let comment = Comment {
        id: "t-001".into(),
        path: "a.rs".into(),
        excerpt: "+new".into(),
        old_start: None,
        old_end: None,
        new_start: Some(2),
        new_end: Some(2),
        anchor_old: None,
        anchor_new: Some(2),
        body: CommentBody::Text("this one crosses the whole pane".into()),
    };
    let mut app = App::new(parse_unified_diff(diff), vec![comment]);
    app.key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
    let mut terminal = Terminal::new(TestBackend::new(120, 12)).unwrap();
    terminal.draw(|frame| app.draw(frame)).unwrap();
    let rows = screen(&terminal, 120, 12);

    let header = rows
        .iter()
        .find(|row| row.contains("@@ -1,2 +1,2 @@"))
        .expect("the hunk header is drawn");
    assert_eq!(
        header.matches('│').count(),
        1,
        "it labels the code rather than being one side of it, so no divider \
         splits it: {header:?}"
    );
    assert!(
        rows.iter().any(|row| row.contains("crosses the whole pane")),
        "and the comment is drawn at all"
    );
}

#[test]
fn each_side_shows_only_its_own_line_number() {
    // Old line 7 was replaced by new line 9: neither number belongs on both
    // sides.
    let diff = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -7 +9 @@\n-old\n+new\n";
    let mut app = App::new(parse_unified_diff(diff), Vec::new());
    app.key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
    let mut terminal = Terminal::new(TestBackend::new(120, 12)).unwrap();
    terminal.draw(|frame| app.draw(frame)).unwrap();
    let rows = screen(&terminal, 120, 12);

    let paired = rows
        .iter()
        .find(|row| row.contains("old") && row.contains("new"))
        .expect("the pair is drawn");
    let divider = paired.find('│').unwrap();
    assert!(paired[..divider].contains('7'), "the old number is on the left");
    assert!(!paired[..divider].contains('9'), "and only the old one");
    assert!(paired[divider..].contains('9'), "the new number is on the right");
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p udiff --bin udiff "split_puts_a_removal\|cross_both_panes\|its_own_line_number"`
Expected: FAIL — the diff still draws one column, so no row holds both words.

- [ ] **Step 3: Let a line carry one side's gutter**

`diff_line` builds a gutter holding both numbers, which is right for the
unified view and wrong for either side of a split one. Give it the choice.

In `udiff/src/app/diff_view.rs`, change the signature and the gutter it builds:

```rust
    /// `side` picks whose line number the gutter carries: `None` for the
    /// unified view, which shows both.
    fn diff_line<'a>(
        &self,
        l: &'a DiffLine,
        p: usize,
        width: usize,
        side: Option<Side>,
    ) -> Line<'a> {
```

Replace the two lines that build `old` and `new` and the span that prints them
with:

```rust
        let number = |value: Option<u32>| value.map_or("    ".into(), |v| format!("{v:>4}"));
        let numbers = match side {
            None => format!("{} {} ", number(l.old), number(l.new)),
            Some(Side::Left) => format!("{} ", number(l.old)),
            Some(Side::Right) => format!("{} ", number(l.new)),
        };
```

and use `numbers` where the old `format!("{old} {new} ")` was. The span count
is unchanged, so `code_start` stays 3 and every caller of `wrap_code_line` and
`crop_code_line` keeps working.

Pass `None` at the two existing call sites in the unified path and in
`line_for_test`.

- [ ] **Step 4: Compose a paired row into one line**

Extend the `super::` import at the top of `udiff/src/app/diff_view.rs` with
`rows::{Row, Side}`, then add to `Renderer`:

```rust
    /// One side of a paired row, already cropped to its half. An empty cell is
    /// drawn as background so the two sides stay level.
    fn split_cell(&self, line: Option<usize>, side: Side, width: usize) -> Vec<Span<'static>> {
        let Some(index) = line else {
            return vec![Span::styled(
                " ".repeat(width),
                Style::default().bg(theme().bg),
            )];
        };
        let file = self.current();
        let built = self.diff_line(&file.lines[index], index, width, Some(side));
        crop_code_line(built, 3, width, self.pane.h_scroll).spans
    }

    /// A paired row is one line across the whole pane, which is what lets a
    /// hunk header and a comment card cross it.
    fn split_row(&self, row: Row, width: usize) -> Line<'static> {
        let left_width = width.saturating_sub(1) / 2;
        let right_width = width.saturating_sub(1 + left_width);
        let mut spans = self.split_cell(row.left, Side::Left, left_width);
        spans.push(Span::styled(
            "\u{2502}",
            Style::default().fg(theme().border).bg(theme().bg),
        ));
        spans.extend(self.split_cell(row.right, Side::Right, right_width));
        Line::from(spans)
    }
```

- [ ] **Step 5: Walk rows instead of lines when the mode is on**

In `draw_diff`, the unified loop walks `file.lines` from `self.pane.scroll`.
Give the split mode its own loop over `self.pane.rows`, placed where the
unified loop is and guarded by the mode. For each row it pushes one line, and
then the comment cards anchored to whichever line the row holds — the same
cards the unified path appends, drawn at full width because a paired row is a
full-width line too.

```rust
        if self.pane.split {
            for row in self.pane.rows.iter().skip(self.pane.scroll) {
                if lines.len() >= height {
                    break;
                }
                let anchor = if row.full_width {
                    row.left
                } else {
                    row.occupant(self.pane.side).or(row.left).or(row.right)
                };
                if row.full_width {
                    let index = row.left.unwrap_or(0);
                    let built = self.diff_line(&file.lines[index], index, a.width as usize, None);
                    lines.push(crop_code_line(built, 3, a.width as usize, 0));
                } else {
                    lines.push(self.split_row(*row, a.width as usize));
                }
                map.push(anchor);
                for (number, comment) in self
                    .session
                    .comments
                    .iter()
                    .filter(|comment| comment.path == file.path)
                    .enumerate()
                    .filter(|(_, comment)| {
                        anchor.is_some() && anchor_position(&file, comment) == anchor
                    })
                {
                    for comment_line in
                        inline_comment_lines(comment, number + 1, a.width as usize)
                    {
                        lines.push(comment_line);
                        map.push(None);
                    }
                }
            }
            self.pane.row_map = map;
            f.render_widget(
                Paragraph::new(lines).style(Style::default().fg(theme().text).bg(theme().bg)),
                a,
            );
            return;
        }
```

Place this after `self.pane.scroll` has been settled and the sticky-hunk block
has run, immediately before the unified loop.

If the borrow checker objects to iterating `self.pane.rows` while calling
`self.split_row`, bind `let rows = self.pane.rows.clone();` first and walk
that — `Row` is `Copy` and the list is one entry per diff line, so the copy is
cheap and buys a shorter borrow.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p udiff --bin udiff "split_puts_a_removal\|cross_both_panes\|its_own_line_number"`
Expected: PASS, 3 tests.

- [ ] **Step 7: Run the whole suite**

Run:
```bash
cargo fmt --all
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
Expected: all pass.

- [ ] **Step 8: Look at it**

Run: `git show HEAD | cargo run -p udiff`
Press `s`. Expected: two columns with a divider, removals level with the
additions that replaced them, each side showing only its own line number, hunk
headers running across, comment cards running across. `j` walks one side and
crosses when it runs out. `w` returns to the unified view. Quit with `q`.

- [ ] **Step 9: Commit**

```bash
git add udiff/src/app/diff_view.rs udiff/src/app/tests.rs
git commit -m "feat(udiff): draw the diff side by side

A rewritten block reads as a before and an after rather than as two runs of
lines. Each side carries only its own line number, since neither number
belongs to both.

A paired row is one line across the whole pane rather than two independent
columns. Drawing the halves separately would leave nothing able to cross
them, and a hunk header and a comment card both have to: they label the code
rather than being one side of it. It also keeps row_map one entry per drawn
row, so the mouse lands where it did before.

Both sides share one horizontal offset. Independent offsets drift apart under
the hand, and there is nothing to compare once they have."
```

---

### Task 3: Document the mode

**Files:**
- Modify: `udiff/src/app/help.rs`
- Modify: `README.md`
- Modify: `ARCHITECTURE.md`
- Modify: `CHANGELOG.md`

**Interfaces:**
- Consumes: the behaviour built in Tasks 1 and 2.
- Produces: nothing code depends on.

- [ ] **Step 1: Add the key to the help**

In `udiff/src/app/help.rs`, in `help_lines`, after the `w` entry:

```rust
        help_line("", "s", "side by side / unified"),
```

- [ ] **Step 2: Mention it in the README**

In `README.md`, in the `## Keys` section, extend the sentence so it reads:

```markdown
Press `?` for the full list. `j`/`k` and the arrows move, `Tab` switches between
the file list and the diff, `b` hides the file list, `s` shows the diff side by
side, `/` filters files, `q` quits.
```

- [ ] **Step 3: Record the module in ARCHITECTURE.md**

In the "Read the code in this order" list, insert after the `diff_pane.rs`
entry and renumber what follows:

```markdown
10. `udiff/src/app/rows.rs` pairs the lines for the side-by-side view.
```

Add a row to the "Where a change belongs" table, directly under the cursor row:

```markdown
| Side-by-side pairing | `udiff/src/app/rows.rs` |
```

- [ ] **Step 4: Record the feature in CHANGELOG.md**

Add to the `### Added` list under `## [Unreleased]`, creating that section if
the previous release closed it:

```markdown
- `s` shows the diff side by side, with each removal level with the addition
  that replaced it. Wrapping turns off while it is on, because aligned rows
  cost one screen row each.
```

- [ ] **Step 5: Verify the docs match the code**

Run:
```bash
grep -n '"s"' udiff/src/app/help.rs
grep -n "rows.rs" ARCHITECTURE.md
grep -n "side by side" README.md CHANGELOG.md
```
Expected: the help names `s`, `ARCHITECTURE.md` mentions the module in both
places, and both the README and the changelog describe the mode.

- [ ] **Step 6: Commit**

```bash
git add udiff/src/app/help.rs README.md ARCHITECTURE.md CHANGELOG.md
git commit -m "docs: describe the side-by-side view"
```
