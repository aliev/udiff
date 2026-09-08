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

/// A drawn row. Headings are not selectable: `selected` counts files, so the
/// cursor cannot land on one and no key needs to step over it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Entry {
    Header(&'static str),
    File(usize),
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
    pub fn open(&mut self, view: &View<'_>) {
        self.open = true;
        // The diff may have been replaced since, so the kept position has to
        // be brought back inside the list it now indexes.
        let last = self.matches(view).len().saturating_sub(1);
        self.selected = self.selected.min(last);
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// The files the query admits, in the order they should be worked
    /// through: what is left before what is done, and within each, what has
    /// been written about before what has not. The diff's own order breaks
    /// ties, so a file never moves for a reason the list does not show.
    pub fn matches(&self, view: &View<'_>) -> Vec<usize> {
        let mut matched: Vec<usize> = view
            .files
            .iter()
            .enumerate()
            .filter(|(_, file)| self.query.is_empty() || fuzzy(&self.query, &file.path))
            .map(|(index, _)| index)
            .collect();
        matched.sort_by_key(|file| {
            (
                view.reviewed_files.contains(file),
                self.comments(*file, view) == 0,
                *file,
            )
        });
        matched
    }

    fn comments(&self, file: usize, view: &View<'_>) -> usize {
        let path = &view.files[file].path;
        view.comments
            .iter()
            .filter(|comment| &comment.path == path)
            .count()
    }

    /// The list as it is drawn: the files, with a heading wherever the group
    /// changes. Headings appear only once the list actually splits — a lone
    /// "to review" over every file at the start of a review says nothing.
    fn entries(&self, view: &View<'_>) -> Vec<Entry> {
        let matched = self.matches(view);
        let reviewed = |file: &usize| view.reviewed_files.contains(file);
        let splits = matched.iter().any(reviewed) && !matched.iter().all(reviewed);
        let mut entries = Vec::new();
        let mut group = None;
        for file in matched {
            let done = view.reviewed_files.contains(&file);
            if splits && group != Some(done) {
                entries.push(Entry::Header(if done { "reviewed" } else { "to review" }));
                group = Some(done);
            }
            entries.push(Entry::File(file));
        }
        entries
    }

    pub fn event(&mut self, key: KeyEvent, view: &View<'_>) -> Action {
        if !self.open {
            return Action::Ignored;
        }
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => self.open = false,
            KeyCode::Enter => {
                let target = self.matches(view).get(self.selected).copied();
                self.open = false;
                if let Some(file) = target {
                    return Action::Open(file);
                }
            }
            KeyCode::Up | KeyCode::BackTab => self.step(-1, view),
            KeyCode::Down | KeyCode::Tab => self.step(1, view),
            KeyCode::Char('p') if control => self.step(-1, view),
            KeyCode::Char('n') if control => self.step(1, view),
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

    fn step(&mut self, delta: isize, view: &View<'_>) {
        let last = self.matches(view).len().saturating_sub(1) as isize;
        self.selected = (self.selected as isize + delta).clamp(0, last.max(0)) as usize;
    }

    pub fn draw(&mut self, frame: &mut Frame, root: Rect, view: &View<'_>) {
        if !self.open {
            return;
        }
        let entries = self.entries(view);
        let width = root.width.saturating_sub(4).min(MAX_WIDTH);
        let rows = u16::try_from(entries.len().max(1)).unwrap_or(u16::MAX);
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
        if entries.is_empty() {
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
        // `selected` counts files; the widget counts drawn rows, and headings
        // sit between them.
        let highlight = entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| matches!(entry, Entry::File(_)))
            .nth(self.selected)
            .map(|(row, _)| row);
        let items: Vec<ListItem> = entries
            .iter()
            .map(|entry| match entry {
                // The widget already reserves the cursor's two columns, so
                // the heading lines up with the paths rather than past them.
                Entry::Header(title) => ListItem::new(Line::from(Span::styled(
                    (*title).to_owned(),
                    Style::default()
                        .fg(theme().muted)
                        .add_modifier(Modifier::BOLD),
                ))),
                Entry::File(file) => ListItem::new(row(*file, view, inner.saturating_sub(2))),
            })
            .collect();
        let mut state = ListState::default().with_selected(highlight);
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

    fn view<'a>(
        files: &'a [FileDiff],
        comments: &'a [Comment],
        reviewed: &'a HashSet<usize>,
    ) -> View<'a> {
        View {
            files,
            comments,
            reviewed_files: reviewed,
        }
    }

    fn plain(files: &[FileDiff]) -> View<'_> {
        View {
            files,
            comments: &[],
            reviewed_files: NOTHING_REVIEWED.get_or_init(HashSet::new),
        }
    }

    static NOTHING_REVIEWED: std::sync::OnceLock<HashSet<usize>> = std::sync::OnceLock::new();

    fn press(picker: &mut Picker, view: &View<'_>, code: KeyCode) -> Action {
        picker.event(KeyEvent::new(code, KeyModifiers::NONE), view)
    }

    fn type_in(picker: &mut Picker, view: &View<'_>, query: &str) {
        for character in query.chars() {
            press(picker, view, KeyCode::Char(character));
        }
    }

    fn comment_on(path: &str) -> Comment {
        Comment {
            id: path.to_owned(),
            path: path.to_owned(),
            excerpt: String::new(),
            old_start: None,
            old_end: None,
            new_start: Some(1),
            new_end: Some(1),
            anchor_old: None,
            anchor_new: Some(1),
            body: crate::comment::CommentBody::Text("look".into()),
        }
    }

    #[test]
    fn a_closed_picker_leaves_every_key_alone() {
        let mut picker = Picker::default();
        assert_eq!(
            press(&mut picker, &plain(&files()), KeyCode::Char('j')),
            Action::Ignored
        );
    }

    #[test]
    fn the_query_narrows_the_list_without_needing_the_letters_adjacent() {
        let files = files();
        let view = plain(&files);
        let mut picker = Picker::default();
        picker.open(&view);
        assert_eq!(picker.matches(&view).len(), 3, "everything, to begin with");

        type_in(&mut picker, &view, "dpane");
        assert_eq!(picker.matches(&view), vec![0]);
    }

    #[test]
    fn typing_returns_the_cursor_to_the_best_match() {
        let files = files();
        let view = plain(&files);
        let mut picker = Picker::default();
        picker.open(&view);
        press(&mut picker, &view, KeyCode::Down);
        press(&mut picker, &view, KeyCode::Down);
        assert_eq!(picker.selected, 2);

        // The list under the cursor is gone; staying at 2 would point at a
        // file the query never matched.
        type_in(&mut picker, &view, "rs");
        assert_eq!(picker.selected, 0);
        assert!(picker.selected < picker.matches(&view).len());
    }

    #[test]
    fn the_cursor_stops_at_both_ends() {
        let files = files();
        let view = plain(&files);
        let mut picker = Picker::default();
        picker.open(&view);
        press(&mut picker, &view, KeyCode::Up);
        assert_eq!(picker.selected, 0, "no wrapping off the top");
        for _ in 0..9 {
            press(&mut picker, &view, KeyCode::Down);
        }
        assert_eq!(picker.selected, 2, "nor off the bottom");
    }

    #[test]
    fn enter_opens_the_file_under_the_cursor_and_closes() {
        let files = files();
        let view = plain(&files);
        let mut picker = Picker::default();
        picker.open(&view);
        type_in(&mut picker, &view, "rows");
        assert_eq!(press(&mut picker, &view, KeyCode::Enter), Action::Open(1));
        assert!(!picker.is_open());
    }

    #[test]
    fn enter_on_nothing_opens_nothing() {
        let files = files();
        let view = plain(&files);
        let mut picker = Picker::default();
        picker.open(&view);
        type_in(&mut picker, &view, "zzz");
        assert!(picker.matches(&view).is_empty());
        assert_eq!(press(&mut picker, &view, KeyCode::Enter), Action::Consumed);
        assert!(!picker.is_open());
    }

    #[test]
    fn escape_closes_without_opening_anything() {
        let files = files();
        let view = plain(&files);
        let mut picker = Picker::default();
        picker.open(&view);
        assert_eq!(press(&mut picker, &view, KeyCode::Esc), Action::Consumed);
        assert!(!picker.is_open());
    }

    #[test]
    fn opening_again_resumes_the_last_search() {
        let files = files();
        let view = plain(&files);
        let mut picker = Picker::default();
        picker.open(&view);
        type_in(&mut picker, &view, "rs");
        press(&mut picker, &view, KeyCode::Down);
        press(&mut picker, &view, KeyCode::Esc);

        picker.open(&view);
        assert_eq!(picker.matches(&view).len(), 2, "the query survived");
        assert_eq!(picker.selected, 1, "and so did the place in the list");

        picker.event(
            KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
            &view,
        );
        assert_eq!(picker.matches(&view).len(), 3, "Ctrl+U starts over");
    }

    #[test]
    fn a_kept_position_is_brought_back_inside_a_shorter_list() {
        let files = files();
        let view = plain(&files);
        let mut picker = Picker::default();
        picker.open(&view);
        press(&mut picker, &view, KeyCode::Down);
        press(&mut picker, &view, KeyCode::Down);
        assert_eq!(picker.selected, 2);
        press(&mut picker, &view, KeyCode::Esc);

        // A watch revision can leave fewer files than the position expects.
        picker.open(&plain(&files[..1]));
        assert_eq!(picker.selected, 0);
    }

    #[test]
    fn what_is_left_comes_before_what_is_done() {
        let files = files();
        let comments = vec![comment_on("src/app/rows.rs"), comment_on("README.md")];
        let reviewed = HashSet::from([2]);
        let picker = Picker::default();

        // README.md is index 2 and reviewed, so it sinks despite its comment;
        // rows.rs is written about, so it rises above the untouched pane.
        assert_eq!(
            picker.matches(&view(&files, &comments, &reviewed)),
            vec![1, 0, 2]
        );
    }

    #[test]
    fn headings_appear_only_once_the_list_splits() {
        let files = files();
        let comments = Vec::new();
        let picker = Picker::default();

        let nothing_done = HashSet::new();
        assert!(
            picker
                .entries(&view(&files, &comments, &nothing_done))
                .iter()
                .all(|entry| matches!(entry, Entry::File(_))),
            "a heading over every file at the start of a review says nothing"
        );

        let some_done = HashSet::from([2]);
        assert_eq!(
            picker.entries(&view(&files, &comments, &some_done)),
            vec![
                Entry::Header("to review"),
                Entry::File(0),
                Entry::File(1),
                Entry::Header("reviewed"),
                Entry::File(2),
            ]
        );
    }

    #[test]
    fn the_cursor_counts_files_and_never_headings() {
        let files = files();
        let comments = Vec::new();
        let reviewed = HashSet::from([0]);
        let view = view(&files, &comments, &reviewed);
        let mut picker = Picker::default();
        picker.open(&view);

        // Two rows down is the third file, not the file after one heading.
        press(&mut picker, &view, KeyCode::Down);
        press(&mut picker, &view, KeyCode::Down);
        assert_eq!(press(&mut picker, &view, KeyCode::Enter), Action::Open(0));
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
