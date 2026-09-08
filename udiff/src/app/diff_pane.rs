use super::{
    Focus, change_map,
    comment_editor::CommentEditor,
    diff_view,
    rows::{Row, Side, pair},
    session::Session,
};
use crate::model::{DiffLine, FileDiff, LineKind};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Frame, layout::Rect};
use std::time::Instant;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VisualMode {
    Character {
        anchor_row: usize,
        anchor_col: usize,
    },
    Line {
        anchor_row: usize,
    },
}

#[derive(Debug, Eq, PartialEq)]
pub enum KeyAction {
    Ignored,
    Consumed,
    Copy(String),
    /// Something worth saying out loud happened — a mode that turned another
    /// one off, say, which is otherwise a silent surprise.
    Notice(&'static str),
}

pub struct DiffPane {
    pub file: usize,
    pub cursor: usize,
    pub scroll: usize,
    pub file_cursors: Vec<usize>,
    pub range_anchor: Option<usize>,
    pub visual_mode: Option<VisualMode>,
    pub visual_col: usize,
    pub vim_command: String,
    pub last_click: Option<(Instant, usize)>,
    pub area: Rect,
    pub row_map: Vec<Option<usize>>,
    /// Soft wrapping. Session-only, like every other view preference here.
    pub wrap: bool,
    /// Columns scrolled past on the left. Always 0 while wrapping.
    pub h_scroll: usize,
    /// Side-by-side rather than unified. Session-only, like every other view
    /// preference here.
    pub split: bool,
    /// Which pane the cursor sits in. Not read while the view is unified.
    pub side: Side,
    /// Pairing for the current file. It does not depend on the mode, so it is
    /// rebuilt only when the file changes.
    pub rows: Vec<Row>,
}

impl DiffPane {
    pub fn new(files: &[FileDiff]) -> Self {
        let first = files
            .first()
            .and_then(|file| {
                file.lines
                    .iter()
                    .position(|line| line.review_line().is_some())
            })
            .unwrap_or(0);
        let count = files.len();
        Self {
            file: 0,
            cursor: first,
            scroll: 0,
            file_cursors: vec![0; count],
            range_anchor: None,
            visual_mode: None,
            visual_col: 0,
            vim_command: String::new(),
            last_click: None,
            area: Rect::default(),
            row_map: Vec::new(),
            wrap: true,
            h_scroll: 0,
            split: false,
            side: Side::Left,
            rows: files
                .first()
                .map(|file| pair(&file.lines))
                .unwrap_or_default(),
        }
    }

    /// Steps from a removal to the addition that replaced it, if there is one.
    /// A suggestion rewrites what is on disk, and what is on disk is the new
    /// side — so offering to write one is more use than refusing.
    pub fn step_to_replacement(&mut self) -> bool {
        let Some(row) = self.rows.get(self.row_of(self.cursor)) else {
            return false;
        };
        let (left, right) = (row.left, row.right);
        // A context line sits on both sides; it is already the new side.
        if left != Some(self.cursor) || right == Some(self.cursor) {
            return false;
        }
        let Some(addition) = right else {
            return false;
        };
        self.cursor = addition;
        self.side = Side::Right;
        self.visual_col = 0;
        true
    }

    /// How the diff is being read belongs to the session, not to a revision.
    /// A batch of edits landing must not quietly drop the mode you are in.
    pub fn adopt_view(&mut self, previous: &Self) {
        self.wrap = previous.wrap;
        self.split = previous.split;
        self.side = previous.side;
        self.h_scroll = previous.h_scroll;
    }

    pub fn current<'a>(&self, files: &'a [FileDiff]) -> &'a FileDiff {
        &files[self.file]
    }

    pub fn active_lines<'a>(&'a self, files: &'a [FileDiff]) -> &'a [DiffLine] {
        &files[self.file].lines
    }

    pub fn draw(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        session: &Session,
        editor: &CommentEditor,
        focus: Focus,
        sidebar_hidden: bool,
    ) {
        diff_view::render(self, frame, area, session, editor, focus, sidebar_hidden);
    }

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
        let Some(row) = self
            .rows
            .get(current.saturating_add(delta).clamp(0, last) as usize)
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
        let Some(line) = self.rows[self.row_of(self.cursor)].occupant(side) else {
            return false;
        };
        self.side = side;
        self.cursor = line;
        true
    }

    pub fn selected_bounds(&self) -> (usize, usize) {
        let anchor = self.range_anchor.unwrap_or(self.cursor);
        (anchor.min(self.cursor), anchor.max(self.cursor))
    }

    pub fn move_cursor(&mut self, delta: isize, files: &[FileDiff]) {
        let maximum = self.active_lines(files).len().saturating_sub(1) as isize;
        self.cursor = (self.cursor as isize + delta).clamp(0, maximum) as usize;
        let column_max = self.active_lines(files)[self.cursor]
            .text
            .chars()
            .count()
            .saturating_sub(1);
        self.visual_col = self.visual_col.min(column_max);
    }

    pub fn jump_to_line(&mut self, number: u32, files: &[FileDiff]) {
        let lines = self.active_lines(files);
        let exact = lines
            .iter()
            .position(|line| line.new == Some(number))
            .or_else(|| lines.iter().position(|line| line.old == Some(number)));
        let nearest = lines
            .iter()
            .enumerate()
            .filter_map(|(position, line)| {
                line.new
                    .or(line.old)
                    .map(|line_number| (position, line_number.abs_diff(number)))
            })
            .min_by_key(|(_, distance)| *distance)
            .map(|(position, _)| position);
        if let Some(position) = exact.or(nearest) {
            self.cursor = position;
        }
    }

    pub fn switch_file(&mut self, file: usize, files: &[FileDiff]) {
        if file >= files.len() {
            return;
        }
        self.file_cursors[self.file] = self.cursor;
        self.file = file;
        self.rows = pair(&files[file].lines);
        let stored = self.file_cursors[file];
        self.cursor = stored.min(self.active_lines(files).len().saturating_sub(1));
        self.scroll = 0;
        self.range_anchor = None;
    }

    pub fn character_selection(
        &self,
        files: &[FileDiff],
        anchor_row: usize,
        anchor_col: usize,
    ) -> String {
        let (first, second) = ((anchor_row, anchor_col), (self.cursor, self.visual_col));
        let ((start_row, start_col), (end_row, end_col)) = if first <= second {
            (first, second)
        } else {
            (second, first)
        };
        (start_row..=end_row)
            .filter_map(|row| {
                let line = &self.active_lines(files)[row];
                if line.kind == LineKind::Meta {
                    return None;
                }
                let length = line.text.chars().count();
                let start = if row == start_row { start_col } else { 0 }.min(length);
                let end = if row == end_row {
                    (end_col + 1).min(length)
                } else {
                    length
                };
                Some(
                    line.text
                        .chars()
                        .skip(start)
                        .take(end.saturating_sub(start))
                        .collect::<String>(),
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn yank_selection(&mut self, files: &[FileDiff]) -> Option<String> {
        let mode = self.visual_mode?;
        let code = match mode {
            VisualMode::Line { anchor_row } => {
                let (start, end) = (anchor_row.min(self.cursor), anchor_row.max(self.cursor));
                self.active_lines(files)[start..=end]
                    .iter()
                    .filter(|line| line.kind != LineKind::Meta)
                    .map(|line| line.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            VisualMode::Character {
                anchor_row,
                anchor_col,
            } => self.character_selection(files, anchor_row, anchor_col),
        };
        if code.is_empty() {
            return None;
        }
        self.visual_mode = None;
        Some(code)
    }

    pub fn key(&mut self, key: KeyEvent, files: &[FileDiff], focused: bool) -> KeyAction {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            let half_page = (self.area.height / 2).max(1) as isize;
            match key.code {
                KeyCode::Char('d') => {
                    self.vim_command.clear();
                    self.move_cursor(half_page, files);
                    return KeyAction::Consumed;
                }
                KeyCode::Char('u') => {
                    self.vim_command.clear();
                    self.move_cursor(-half_page, files);
                    return KeyAction::Consumed;
                }
                _ => {}
            }
        }
        if focused {
            match key.code {
                KeyCode::Char(character) if character.is_ascii_digit() => {
                    if !self.vim_command.chars().all(|part| part.is_ascii_digit()) {
                        self.vim_command.clear();
                    }
                    self.vim_command.push(character);
                    return KeyAction::Consumed;
                }
                KeyCode::Char('g') => {
                    if self.vim_command.ends_with('g') {
                        if let Ok(line) = self.vim_command.trim_end_matches('g').parse::<u32>() {
                            self.jump_to_line(line, files);
                        } else {
                            self.cursor = 0;
                        }
                        self.vim_command.clear();
                    } else if self.vim_command.chars().all(|part| part.is_ascii_digit()) {
                        self.vim_command.push('g');
                    } else {
                        self.vim_command.clear();
                    }
                    return KeyAction::Consumed;
                }
                _ => {}
            }
        }
        if !matches!(key.code, KeyCode::Esc) {
            self.vim_command.clear();
        }
        match key.code {
            KeyCode::Char('G') => self.cursor = self.active_lines(files).len().saturating_sub(1),
            KeyCode::Char('j') | KeyCode::Down if focused => {
                if self.split {
                    self.step_row(1, files);
                } else {
                    self.move_cursor(1, files);
                }
            }
            KeyCode::Char('k') | KeyCode::Up if focused => {
                if self.split {
                    self.step_row(-1, files);
                } else {
                    self.move_cursor(-1, files);
                }
            }
            KeyCode::Char('w') if focused => {
                self.wrap = !self.wrap;
                // Nothing is off to the left once the line folds instead.
                if self.wrap {
                    self.h_scroll = 0;
                    if std::mem::take(&mut self.split) {
                        return KeyAction::Notice("wrapped rows cannot line up · side by side off");
                    }
                }
            }
            KeyCode::Char('s') if focused => {
                self.split = !self.split;
                if self.split {
                    // Aligned rows cost one screen row each, so a folded line
                    // would drift the two sides apart.
                    self.h_scroll = 0;
                    self.side = self.side_of(self.cursor);
                    if std::mem::take(&mut self.wrap) {
                        return KeyAction::Notice("wrapped rows cannot line up · wrapping off");
                    }
                }
            }
            // A file with many changes is mostly context to scroll past, so
            // the changes get a motion of their own.
            KeyCode::Char('n' | 'N') if focused => {
                let forward = key.code == KeyCode::Char('n');
                match change_map::jump(self.active_lines(files), self.cursor, forward) {
                    Some(line) => {
                        self.cursor = line;
                        self.visual_col = 0;
                        if self.split {
                            self.side = self.side_of(line);
                        }
                    }
                    None => {
                        return KeyAction::Notice(if forward {
                            "no change after this one"
                        } else {
                            "no change before this one"
                        });
                    }
                }
            }
            // Vim's own line motions: `^` is the first non-blank, which on
            // indented code is the character you actually want.
            KeyCode::Char('h') | KeyCode::Left
                if focused && self.visual_col == 0 && self.cross(Side::Left) => {}
            KeyCode::Char('^') if focused => {
                let text = &self.active_lines(files)[self.cursor].text;
                self.visual_col = text
                    .chars()
                    .position(|character| !character.is_whitespace())
                    .unwrap_or(0);
            }
            KeyCode::Char('$') if focused => {
                self.visual_col = self.active_lines(files)[self.cursor]
                    .text
                    .chars()
                    .count()
                    .saturating_sub(1);
            }
            KeyCode::Char('h') | KeyCode::Left if focused => {
                self.visual_col = self.visual_col.saturating_sub(1)
            }
            KeyCode::Char('l') | KeyCode::Right if focused => {
                let maximum = self.active_lines(files)[self.cursor]
                    .text
                    .chars()
                    .count()
                    .saturating_sub(1);
                if self.visual_col < maximum || !self.cross(Side::Right) {
                    self.visual_col = (self.visual_col + 1).min(maximum);
                }
            }
            KeyCode::Char('c') if focused => {
                self.visual_mode = None;
                self.range_anchor = self.range_anchor.is_none().then_some(self.cursor);
            }
            KeyCode::Char('v') if focused => {
                self.range_anchor = None;
                self.visual_mode = if matches!(self.visual_mode, Some(VisualMode::Character { .. }))
                {
                    None
                } else {
                    Some(VisualMode::Character {
                        anchor_row: self.cursor,
                        anchor_col: self.visual_col,
                    })
                };
            }
            KeyCode::Char('V') if focused => {
                self.range_anchor = None;
                self.visual_mode = if matches!(self.visual_mode, Some(VisualMode::Line { .. })) {
                    None
                } else {
                    Some(VisualMode::Line {
                        anchor_row: self.cursor,
                    })
                };
            }
            KeyCode::Char('y') if focused => {
                return self
                    .yank_selection(files)
                    .map_or(KeyAction::Consumed, KeyAction::Copy);
            }
            KeyCode::Esc if focused => {
                self.range_anchor = None;
                self.visual_mode = None;
                self.vim_command.clear();
            }
            _ => return KeyAction::Ignored,
        }
        KeyAction::Consumed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::parse_unified_diff;

    #[test]
    fn pane_owns_independent_cursor_state_for_each_file() {
        let files = parse_unified_diff(
            "diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -1 +1 @@\n-a\n+b\ndiff --git a/b b/b\n--- a/b\n+++ b/b\n@@ -1 +1 @@\n-a\n+b\n",
        );
        let pane = DiffPane::new(&files);
        assert_eq!(pane.file_cursors, vec![0, 0]);
        assert_eq!(pane.current(&files).path, "a");
    }
}
