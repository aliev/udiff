//! The file list `p` opens.
//!
//! The explorer is a panel: it costs a column of the terminal for as long as
//! it is up, which in a split terminal is most of the reading width. This
//! costs nothing until asked for, and answers the only question the explorer
//! was being kept open to answer — which file next.

use super::{render::file_status_spans, search::fuzzy};
use crate::{comment::Comment, model::FileDiff, theme::theme};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
};
use std::collections::HashSet;
use unicode_width::UnicodeWidthStr;

/// The border, and the query line under it.
const CHROME_ROWS: u16 = 3;
const MAX_WIDTH: u16 = 72;

#[derive(Default)]
pub struct Picker {
    open: bool,
    query: String,
    selected: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    Ignored,
    Consumed,
    Open(usize),
}

pub struct View<'a> {
    pub files: &'a [FileDiff],
    pub comments: &'a [Comment],
    pub reviewed_files: &'a HashSet<usize>,
}

impl Picker {
    /// Opens where it was left. Narrowing to a file is work, and a reviewer
    /// coming back to the list is usually going somewhere near the last
    /// place — throwing the query away would charge for that work twice.
    /// `Ctrl+U` clears it when the next search is unrelated.
    pub fn open(&mut self, files: &[FileDiff]) {
        self.open = true;
        // The diff may have been replaced since, so the kept position has to
        // be brought back inside the list it now indexes.
        let last = self.matches(files).len().saturating_sub(1);
        self.selected = self.selected.min(last);
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// The files the query admits, in the order the diff lists them.
    pub fn matches(&self, files: &[FileDiff]) -> Vec<usize> {
        files
            .iter()
            .enumerate()
            .filter(|(_, file)| self.query.is_empty() || fuzzy(&self.query, &file.path))
            .map(|(index, _)| index)
            .collect()
    }

    pub fn event(&mut self, key: KeyEvent, files: &[FileDiff]) -> Action {
        if !self.open {
            return Action::Ignored;
        }
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => self.open = false,
            KeyCode::Enter => {
                let target = self.matches(files).get(self.selected).copied();
                self.open = false;
                if let Some(file) = target {
                    return Action::Open(file);
                }
            }
            KeyCode::Up | KeyCode::BackTab => self.step(-1, files),
            KeyCode::Down | KeyCode::Tab => self.step(1, files),
            KeyCode::Char('p') if control => self.step(-1, files),
            KeyCode::Char('n') if control => self.step(1, files),
            KeyCode::Backspace => {
                self.query.pop();
                self.selected = 0;
            }
            KeyCode::Char('u') if control => {
                self.query.clear();
                self.selected = 0;
            }
            KeyCode::Char(character)
                if !key.modifiers.intersects(
                    KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER,
                ) =>
            {
                self.query.push(character);
                // Another character means another list. Holding the old
                // position would leave the cursor on an unrelated file, so it
                // returns to the best match, as every finder does.
                self.selected = 0;
            }
            _ => {}
        }
        Action::Consumed
    }

    fn step(&mut self, delta: isize, files: &[FileDiff]) {
        let last = self.matches(files).len().saturating_sub(1) as isize;
        self.selected = (self.selected as isize + delta).clamp(0, last.max(0)) as usize;
    }

    pub fn draw(&mut self, frame: &mut Frame, root: Rect, view: &View<'_>) {
        if !self.open {
            return;
        }
        let matches = self.matches(view.files);
        let width = root.width.saturating_sub(4).min(MAX_WIDTH);
        let rows = u16::try_from(matches.len().max(1)).unwrap_or(u16::MAX);
        let height = root
            .height
            .saturating_sub(2)
            .min(rows.saturating_add(CHROME_ROWS));
        let area = Rect {
            x: root.x + root.width.saturating_sub(width) / 2,
            y: root.y + root.height.saturating_sub(height) / 2,
            width,
            height,
        };
        let inner = usize::from(width.saturating_sub(2));
        frame.render_widget(Clear, area);
        frame.render_widget(
            Block::default()
                .title(" Files ")
                .title_bottom(Line::from(Span::styled(
                    " Enter open \u{b7} Esc close ",
                    Style::default().fg(theme().muted),
                )))
                .borders(Borders::ALL)
                .border_type(ratatui::widgets::BorderType::Rounded)
                .border_style(Style::default().fg(theme().blue))
                .style(Style::default().fg(theme().text).bg(theme().surface)),
            area,
        );
        let body = Rect {
            x: area.x + 1,
            y: area.y + 1,
            width: width.saturating_sub(2),
            height: height.saturating_sub(2),
        };
        if body.height == 0 {
            return;
        }
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    "\u{203a} ",
                    Style::default()
                        .fg(theme().blue)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(self.query.clone(), Style::default().fg(theme().text)),
                Span::styled(" ", theme().caret()),
            ]))
            .style(Style::default().bg(theme().surface)),
            Rect { height: 1, ..body },
        );
        let list = Rect {
            y: body.y + 1,
            height: body.height.saturating_sub(1),
            ..body
        };
        if list.height == 0 {
            return;
        }
        if matches.is_empty() {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "  no match",
                    Style::default().fg(theme().muted),
                )))
                .style(Style::default().bg(theme().surface)),
                list,
            );
            return;
        }
        let items: Vec<ListItem> = matches
            .iter()
            .map(|file| ListItem::new(row(*file, view, inner.saturating_sub(2))))
            .collect();
        let mut state = ListState::default().with_selected(Some(self.selected));
        frame.render_stateful_widget(
            List::new(items)
                .style(Style::default().bg(theme().surface))
                .highlight_style(theme().selected(Style::default().fg(theme().text)))
                .highlight_symbol("\u{258c} "),
            list,
            &mut state,
        );
    }
}

/// One file, with what the explorer was being kept open to show: how big the
/// change is, and whether it has been dealt with.
fn row(file: usize, view: &View<'_>, width: usize) -> Line<'static> {
    let diff = &view.files[file];
    let comments = view
        .comments
        .iter()
        .filter(|comment| comment.path == diff.path)
        .count();
    let mut tail = Vec::new();
    if view.reviewed_files.contains(&file) {
        tail.push(Span::styled(
            " \u{2713}",
            Style::default().fg(theme().green),
        ));
    }
    if comments > 0 {
        tail.push(Span::styled(
            format!(" \u{b7} {comments}"),
            Style::default().fg(theme().comment),
        ));
    }
    tail.push(Span::styled(
        format!(" +{}", diff.additions()),
        Style::default().fg(theme().green),
    ));
    tail.push(Span::styled(
        format!(" \u{2212}{}", diff.deletions()),
        Style::default().fg(theme().red),
    ));
    let tail_width: usize = tail.iter().map(|span| span.content.width()).sum();
    let mut spans = file_status_spans(diff.status);
    let room = width.saturating_sub(2 + tail_width);
    let path = ellipsised(&diff.path, room);
    let pad = room.saturating_sub(path.width());
    spans.push(Span::styled(path, Style::default().fg(theme().text)));
    spans.push(Span::raw(" ".repeat(pad)));
    spans.extend(tail);
    Line::from(spans)
}

/// Paths are long and their ends are what tells them apart, so the front goes.
fn ellipsised(path: &str, width: usize) -> String {
    if path.width() <= width {
        return path.to_owned();
    }
    let mut out = String::new();
    let mut taken = 0;
    for character in path.chars().rev() {
        let step = UnicodeWidthStr::width(character.encode_utf8(&mut [0u8; 4]) as &str);
        if taken + step + 1 > width {
            break;
        }
        taken += step;
        out.insert(0, character);
    }
    format!("\u{2026}{out}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::parse_unified_diff;
    use crossterm::event::KeyEvent;

    fn files() -> Vec<FileDiff> {
        let mut text = String::new();
        for path in ["src/app/diff_pane.rs", "src/app/rows.rs", "README.md"] {
            text.push_str(&format!(
                "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -1 +1 @@\n+one\n"
            ));
        }
        parse_unified_diff(&text)
    }

    fn press(picker: &mut Picker, files: &[FileDiff], code: KeyCode) -> Action {
        picker.event(KeyEvent::new(code, KeyModifiers::NONE), files)
    }

    fn type_in(picker: &mut Picker, files: &[FileDiff], query: &str) {
        for character in query.chars() {
            press(picker, files, KeyCode::Char(character));
        }
    }

    #[test]
    fn a_closed_picker_leaves_every_key_alone() {
        let mut picker = Picker::default();
        assert_eq!(
            press(&mut picker, &files(), KeyCode::Char('j')),
            Action::Ignored
        );
    }

    #[test]
    fn the_query_narrows_the_list_without_needing_the_letters_adjacent() {
        let files = files();
        let mut picker = Picker::default();
        picker.open(&files);
        assert_eq!(picker.matches(&files).len(), 3, "everything, to begin with");

        type_in(&mut picker, &files, "dpane");
        assert_eq!(picker.matches(&files), vec![0]);
    }

    #[test]
    fn typing_returns_the_cursor_to_the_best_match() {
        let files = files();
        let mut picker = Picker::default();
        picker.open(&files);
        press(&mut picker, &files, KeyCode::Down);
        press(&mut picker, &files, KeyCode::Down);
        assert_eq!(picker.selected, 2);

        // The list under the cursor is gone; staying at 2 would point at a
        // file the query never matched.
        type_in(&mut picker, &files, "rs");
        assert_eq!(picker.selected, 0);
        assert!(picker.selected < picker.matches(&files).len());
    }

    #[test]
    fn the_cursor_stops_at_both_ends() {
        let files = files();
        let mut picker = Picker::default();
        picker.open(&files);
        press(&mut picker, &files, KeyCode::Up);
        assert_eq!(picker.selected, 0, "no wrapping off the top");
        for _ in 0..9 {
            press(&mut picker, &files, KeyCode::Down);
        }
        assert_eq!(picker.selected, 2, "nor off the bottom");
    }

    #[test]
    fn enter_opens_the_file_under_the_cursor_and_closes() {
        let files = files();
        let mut picker = Picker::default();
        picker.open(&files);
        type_in(&mut picker, &files, "rows");
        assert_eq!(press(&mut picker, &files, KeyCode::Enter), Action::Open(1));
        assert!(!picker.is_open());
    }

    #[test]
    fn enter_on_nothing_opens_nothing() {
        let files = files();
        let mut picker = Picker::default();
        picker.open(&files);
        type_in(&mut picker, &files, "zzz");
        assert!(picker.matches(&files).is_empty());
        assert_eq!(press(&mut picker, &files, KeyCode::Enter), Action::Consumed);
        assert!(!picker.is_open());
    }

    #[test]
    fn escape_closes_without_opening_anything() {
        let files = files();
        let mut picker = Picker::default();
        picker.open(&files);
        assert_eq!(press(&mut picker, &files, KeyCode::Esc), Action::Consumed);
        assert!(!picker.is_open());
    }

    #[test]
    fn opening_again_resumes_the_last_search() {
        let files = files();
        let mut picker = Picker::default();
        picker.open(&files);
        type_in(&mut picker, &files, "rs");
        press(&mut picker, &files, KeyCode::Down);
        press(&mut picker, &files, KeyCode::Esc);

        picker.open(&files);
        assert_eq!(picker.matches(&files).len(), 2, "the query survived");
        assert_eq!(picker.selected, 1, "and so did the place in the list");

        picker.event(
            KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
            &files,
        );
        assert_eq!(picker.matches(&files).len(), 3, "Ctrl+U starts over");
    }

    #[test]
    fn a_kept_position_is_brought_back_inside_a_shorter_list() {
        let files = files();
        let mut picker = Picker::default();
        picker.open(&files);
        press(&mut picker, &files, KeyCode::Down);
        press(&mut picker, &files, KeyCode::Down);
        assert_eq!(picker.selected, 2);
        press(&mut picker, &files, KeyCode::Esc);

        // A watch revision can leave fewer files than the position expects.
        picker.open(&files[..1]);
        assert_eq!(picker.selected, 0);
    }

    #[test]
    fn a_long_path_loses_its_front_rather_than_its_name() {
        assert_eq!(
            ellipsised("udiff/src/app/diff_pane.rs", 14),
            "\u{2026}/diff_pane.rs"
        );
        assert_eq!(ellipsised("short.rs", 14), "short.rs");
    }
}
