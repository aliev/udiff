# Split view

μdiff shows a unified diff: removals and additions in one column, in the order
the patch lists them. A side-by-side view puts what was there on the left and
what replaced it on the right, aligned, so a rewritten block can be read as a
before and an after rather than as two runs of lines.

## Scope

In scope: a paired row model, a cursor that knows which side it is on,
navigation across and between the panes, and rendering two gutters with a
divider.

Out of scope: connector lines between the panes, per-side horizontal
scrolling, and any setting that survives the session. Each is a separate
decision this design does not foreclose.

## What decides the shape

Two facts drive everything below.

**Alignment costs one screen row per diff row.** If a line folds to two rows on
the left and three on the right, the sides drift apart and the view stops being
a comparison. So split and soft wrapping are mutually exclusive: turning on one
turns off the other. This is a property of the problem, not of the
implementation.

**The cursor stays an index into the file's lines.** Every review behaviour —
comments, suggestions, ranges, yanking, anchoring — addresses lines by index,
and none of it needs to change. The alternative, making the cursor address a
row, would put an empty cell into the cursor's domain and force every consumer
to answer what a comment on nothing means. Pairing is used by navigation and
rendering only.

## Pairing

A new `udiff/src/app/rows.rs` turns a line list into rows. It depends on
`DiffLine` alone and knows nothing about the screen.

```rust
pub enum Side { Left, Right }

pub struct Row {
    pub left: Option<usize>,
    pub right: Option<usize>,
    pub full_width: bool,
}

pub fn pair(lines: &[DiffLine]) -> Vec<Row>
```

The rules:

- A context line occupies both sides of one row, with the same index.
- A run of removals followed immediately by a run of additions is zipped
  position by position; the shorter run leaves empty cells behind.
- Removals with no additions after them occupy the left alone; additions with
  no removals before them occupy the right alone.
- Hunk headers and metadata lines are marked `full_width` and carry their index
  on the left.

## Cursor and navigation

`DiffPane` gains `split: bool`, `side: Side`, and the rows for the current file
and mode, rebuilt whenever either changes. `cursor` keeps its meaning. `side`
means nothing while the view is unified and is not read there.

`j` and `k` find the row holding the current line, step to the neighbouring
row, and take that row's occupant on the current side. When that side is empty
there, they take the other one and move `side` with them, so walking down a run
of removals continues into the additions that replace it rather than stopping.
A `full_width` row has one occupant and leaves `side` alone.

`h` at column zero and `l` at the last column cross to the other pane when it
is occupied. No new key is needed: with wrapping off, `l` already stops at the
end of a line, which makes the edge a natural place to step across.

`s` toggles the mode. Because the modes exclude each other, `s` turns wrapping
off and `w` turns split off.

Entering split puts the cursor on the side its line already belongs to: a
removal on the left, an addition on the right, anything else on the left.
Leaving split changes nothing — the line the cursor was on is the line it
stays on, which is what makes the toggle safe to press while reading.

## Rendering

The diff pane divides into a left column, a one-column divider, and a right
column. Each side carries its own gutter — review mark, line number, change
marker — in eight columns, so a 130-column terminal leaves 38 columns of code
per side.

Rows marked `full_width`, and the comment and suggestion cards, are drawn
across the whole pane as they are today. The horizontal offset is shared by
both sides; independent offsets drift apart under the hand.

## What does not change

Review state and everything that reads it. Comments, suggestions, ranges,
`y`, `Shift+Y`, the file tree, and watch mode all address lines by index, and
those indices keep their meaning in both modes.

## Testing

`pair` is pure and is tested as a table: context only, equal runs, a longer
left, a longer right, additions with nothing removed, removals with nothing
added, and hunk headers.

Navigation is tested through the pane: `j` walks one side, crosses when that
side runs out, and `l` at the end of a line steps across.

Rendering is tested on a known width: both gutters present, the divider in
place, a hunk header spanning, and a comment card spanning.

Two tests cover the exclusion: `s` leaves wrapping off, and `w` leaves split
off. One covers continuity: toggling `s` twice keeps the cursor on the line it
started on, and entering split from a removal lands on the left.

## Documentation

`README.md` gains `s` where the other view keys are described, and
`ARCHITECTURE.md` gains `rows.rs` in its reading order and a row in "Where a
change belongs". `CHANGELOG.md` records the mode under the unreleased entry.
