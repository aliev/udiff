//! Where the changes are in the file, at the resolution of one screen row.
//!
//! Two readings of the same data: a band per row, for the column drawn beside
//! the scrollbar, and the start of each change block, for jumping between
//! them. The scrollbar says where the view is; this says where the work is.

use crate::model::{DiffLine, LineKind};

/// What one cell of the map stands for. A cell covers many lines, so it
/// reports which kinds are present rather than a single kind.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct Band {
    pub removed: bool,
    pub added: bool,
}

impl Band {
    /// The glyph for a map drawn without colour, where the shape has to carry
    /// both channels on its own. The monochrome palette is `Color::Reset`
    /// throughout, so there a colour says nothing at all.
    ///
    /// An unchanged band draws nothing. The scrollbar beside it already holds
    /// a rule down the edge; a second one would bury the marks in it.
    pub(super) fn glyph(self) -> &'static str {
        match (self.removed, self.added) {
            (true, true) => "\u{2588}",
            (true, false) => "\u{258c}",
            (false, true) => "\u{2590}",
            (false, false) => " ",
        }
    }
}

/// One band per screen row, each covering an equal share of the file.
pub(super) fn bands(lines: &[DiffLine], height: usize) -> Vec<Band> {
    if height == 0 {
        return Vec::new();
    }
    (0..height)
        .map(|row| {
            let start = row * lines.len() / height;
            let end = (row + 1) * lines.len() / height;
            lines[start..end]
                .iter()
                .fold(Band::default(), |band, line| Band {
                    removed: band.removed || line.kind == LineKind::Remove,
                    added: band.added || line.kind == LineKind::Add,
                })
        })
        .collect()
}

/// The first line of every change block, in order.
///
/// A removal and the addition that replaces it are one edit, so a run of
/// either kind continues the same block: stopping twice in the middle of one
/// replacement is not what "jump to the next change" means.
fn block_starts(lines: &[DiffLine]) -> Vec<usize> {
    let changed = |line: &DiffLine| matches!(line.kind, LineKind::Add | LineKind::Remove);
    lines
        .iter()
        .enumerate()
        .filter(|(position, line)| {
            changed(line) && (*position == 0 || !changed(&lines[position - 1]))
        })
        .map(|(position, _)| position)
        .collect()
}

/// The block to jump to from `cursor`, or `None` when there is none that way.
///
/// Landing anywhere inside a block counts as being on it, so going back from
/// the middle of one reaches the block before rather than its own first line.
pub(super) fn jump(lines: &[DiffLine], cursor: usize, forward: bool) -> Option<usize> {
    let starts = block_starts(lines);
    let here = starts
        .iter()
        .rposition(|start| *start <= cursor)
        .filter(|index| {
            lines[starts[*index]..=cursor]
                .iter()
                .all(|line| matches!(line.kind, LineKind::Add | LineKind::Remove))
        });
    match (forward, here) {
        (true, Some(index)) => starts.get(index + 1).copied(),
        (true, None) => starts.iter().find(|start| **start > cursor).copied(),
        (false, Some(index)) => index.checked_sub(1).map(|index| starts[index]),
        (false, None) => starts.iter().rev().find(|start| **start < cursor).copied(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::parse_unified_diff;

    /// `c` context, `-` removal, `+` addition — one character per line.
    fn diff(shape: &str) -> Vec<DiffLine> {
        let body: String = shape
            .chars()
            .map(|kind| match kind {
                '-' => "-old\n".to_string(),
                '+' => "+new\n".to_string(),
                _ => " same\n".to_string(),
            })
            .collect();
        let text = format!("diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n{body}");
        let mut lines = parse_unified_diff(&text).remove(0).lines;
        // Drop the file and hunk headers the parser puts in front, so the
        // shape string maps to positions one for one.
        lines.retain(|line| !matches!(line.kind, LineKind::Hunk | LineKind::Meta));
        lines
    }

    #[test]
    fn a_band_reports_every_kind_the_lines_it_covers_hold() {
        // Six lines into three bands: two lines each.
        let map = bands(&diff("cc-+cc"), 3);
        assert_eq!(map[0], Band::default(), "context only");
        assert_eq!(
            map[1],
            Band {
                removed: true,
                added: true
            },
            "a replacement, both halves"
        );
        assert_eq!(map[2], Band::default());
    }

    #[test]
    fn without_colour_the_shape_alone_tells_the_kinds_apart() {
        let glyphs: Vec<_> = bands(&diff("c-+-"), 4)
            .into_iter()
            .map(Band::glyph)
            .collect();
        assert_eq!(glyphs, [" ", "\u{258c}", "\u{2590}", "\u{258c}"]);
    }

    #[test]
    fn a_replacement_is_one_block_rather_than_two() {
        assert_eq!(block_starts(&diff("c--++c-c")), vec![1, 6]);
    }

    #[test]
    fn jumping_leaves_the_block_the_cursor_is_inside() {
        let lines = diff("c-+cccc-c");
        assert_eq!(jump(&lines, 0, true), Some(1));
        assert_eq!(jump(&lines, 2, true), Some(7), "not back to this block");
        assert_eq!(jump(&lines, 7, true), None, "nothing further on");

        assert_eq!(jump(&lines, 7, false), Some(1));
        assert_eq!(jump(&lines, 4, false), Some(1), "from between blocks");
        assert_eq!(jump(&lines, 1, false), None);
    }
}
