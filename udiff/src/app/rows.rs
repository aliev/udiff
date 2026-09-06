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
                // Only additions that follow removals immediately are the other
                // half of the same change; anything between them makes two runs
                // that stand on their own.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::parse_unified_diff;

    fn rows_of(diff: &str) -> Vec<Row> {
        pair(&parse_unified_diff(diff)[0].lines)
    }

    #[test]
    fn a_replaced_line_shares_its_row_with_what_replaced_it() {
        let rows = rows_of("--- a\n+++ b\n@@ -1,2 +1,2 @@\n ctx\n-gone\n+kept\n");
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
