use super::{Focus, comment_editor::CommentEditor, diff_view, session::Session};
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileViewAction {
    None,
    Request(usize),
    Notice(&'static str),
}

pub struct DiffPane {
    pub file: usize,
    pub cursor: usize,
    pub scroll: usize,
    pub file_cursors: Vec<usize>,
    pub file_view_cursors: Vec<usize>,
    pub file_views: Vec<Option<Vec<DiffLine>>>,
    pub file_views_loaded: Vec<bool>,
    pub file_view: bool,
    pub range_anchor: Option<usize>,
    pub visual_mode: Option<VisualMode>,
    pub visual_col: usize,
    pub vim_command: String,
    pub last_click: Option<(Instant, usize)>,
    pub area: Rect,
    pub row_map: Vec<Option<usize>>,
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
            file_view_cursors: vec![0; count],
            file_views: vec![None; count],
            file_views_loaded: vec![false; count],
            file_view: false,
            range_anchor: None,
            visual_mode: None,
            visual_col: 0,
            vim_command: String::new(),
            last_click: None,
            area: Rect::default(),
            row_map: Vec::new(),
        }
    }

    pub fn current<'a>(&self, files: &'a [FileDiff]) -> &'a FileDiff {
        &files[self.file]
    }

    pub fn active_lines<'a>(&'a self, files: &'a [FileDiff]) -> &'a [DiffLine] {
        if self.file_view {
            self.file_views[self.file].as_deref().unwrap_or_default()
        } else {
            &files[self.file].lines
        }
    }

    pub fn draw(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        session: &Session,
        editor: &CommentEditor,
        focus: Focus,
    ) {
        diff_view::render(self, frame, area, session, editor, focus);
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

    pub fn switch_file(&mut self, file: usize, files: &[FileDiff]) -> bool {
        if file >= files.len() {
            return false;
        }
        if self.file_view {
            self.file_view_cursors[self.file] = self.cursor;
        } else {
            self.file_cursors[self.file] = self.cursor;
        }
        self.file = file;
        let lost_file_view = self.file_view && self.file_views[file].is_none();
        if lost_file_view {
            self.file_view = false;
        }
        let stored = if self.file_view {
            self.file_view_cursors[file]
        } else {
            self.file_cursors[file]
        };
        self.cursor = stored.min(self.active_lines(files).len().saturating_sub(1));
        self.scroll = 0;
        self.range_anchor = None;
        lost_file_view
    }

    pub fn request_or_toggle_file_view(&mut self, files: &[FileDiff]) -> FileViewAction {
        if !self.file_view && !self.file_views_loaded[self.file] {
            return FileViewAction::Request(self.file);
        }
        self.toggle_file_view(files)
    }

    pub fn finish_file_view_load(
        &mut self,
        file: usize,
        lines: Option<Vec<DiffLine>>,
        files: &[FileDiff],
    ) -> FileViewAction {
        if file >= files.len() {
            return FileViewAction::None;
        }
        self.file_views[file] = lines;
        self.file_views_loaded[file] = true;
        if file != self.file {
            return FileViewAction::None;
        }
        if self.file_views[file].is_some() {
            self.toggle_file_view(files)
        } else {
            FileViewAction::Notice("full file unavailable for this diff")
        }
    }

    pub fn toggle_file_view(&mut self, files: &[FileDiff]) -> FileViewAction {
        self.range_anchor = None;
        self.visual_mode = None;
        self.visual_col = 0;
        if self.file_view {
            self.file_view_cursors[self.file] = self.cursor;
            let line = self.cursor.saturating_add(1) as u32;
            self.file_view = false;
            self.cursor =
                self.file_cursors[self.file].min(files[self.file].lines.len().saturating_sub(1));
            self.jump_to_line(line, files);
            self.scroll = 0;
            return FileViewAction::Notice("diff view");
        }
        let Some(lines) = self.file_views[self.file].as_ref() else {
            return FileViewAction::Notice("full file unavailable for this diff");
        };
        self.file_cursors[self.file] = self.cursor;
        let target = files[self.file].lines[self.cursor]
            .review_line()
            .unwrap_or(1)
            .saturating_sub(1) as usize;
        self.file_view = true;
        self.cursor = target.min(lines.len().saturating_sub(1));
        self.file_view_cursors[self.file] = self.cursor;
        self.scroll = self.cursor.saturating_sub(3);
        FileViewAction::Notice("full file view · o return to diff")
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
            KeyCode::Char('j') | KeyCode::Down if focused => self.move_cursor(1, files),
            KeyCode::Char('k') | KeyCode::Up if focused => self.move_cursor(-1, files),
            KeyCode::Char('h') | KeyCode::Left if focused => {
                self.visual_col = self.visual_col.saturating_sub(1)
            }
            KeyCode::Char('l') | KeyCode::Right if focused => {
                let maximum = self.active_lines(files)[self.cursor]
                    .text
                    .chars()
                    .count()
                    .saturating_sub(1);
                self.visual_col = (self.visual_col + 1).min(maximum);
            }
            KeyCode::Char('c') if focused && !self.file_view => {
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
        assert!(!pane.file_view);
    }
}
