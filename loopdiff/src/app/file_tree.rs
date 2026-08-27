use super::{
    BORDER, MUTED, SELECT_BG, SURFACE, TEXT,
    render::{crop_spans, file_status_spans},
    search::fuzzy,
};
use crate::{comment::Comment, model::FileDiff};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
};
use std::collections::HashSet;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    File(usize),
    Comment { file: usize, note: usize },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchAction {
    None,
    Cancel,
    Accept(Option<usize>),
}

#[derive(Clone)]
struct Entry {
    label: String,
    depth: usize,
    target: Option<Target>,
}

#[derive(Default)]
pub struct FileTree {
    filter: String,
    restore_filter: String,
    no_match: bool,
    area: Rect,
    row_map: Vec<Option<Target>>,
    scroll_x: usize,
    scroll_y: usize,
    follow_selection: bool,
    selection: Option<Target>,
}

pub struct View<'a> {
    pub files: &'a [FileDiff],
    pub comments: &'a [Comment],
    pub viewed_files: &'a HashSet<usize>,
    pub current_file: usize,
    pub active_comment: Option<usize>,
    pub focused: bool,
}

impl FileTree {
    pub fn filter(&self) -> &str {
        &self.filter
    }

    pub fn no_match(&self) -> bool {
        self.no_match
    }

    #[cfg(test)]
    pub fn selection(&self) -> Option<Target> {
        self.selection
    }

    pub fn select(&mut self, target: Option<Target>, follow: bool) {
        self.selection = target;
        self.follow_selection = follow;
    }

    pub fn contains(&self, column: u16, row: u16) -> bool {
        self.area.contains((column, row).into())
    }

    pub fn scroll_vertical(&mut self, delta: isize) {
        self.scroll_y = self.scroll_y.saturating_add_signed(delta);
        self.follow_selection = false;
    }

    pub fn target_at(&self, terminal_row: u16) -> Option<Target> {
        let row = terminal_row.saturating_sub(self.area.y) as usize;
        self.row_map.get(row).copied().flatten()
    }

    #[cfg(test)]
    pub fn set_filter(&mut self, filter: impl Into<String>) {
        self.filter = filter.into();
    }

    #[cfg(test)]
    pub fn scroll_y(&self) -> usize {
        self.scroll_y
    }

    #[cfg(test)]
    pub fn visible_targets(&self) -> &[Option<Target>] {
        &self.row_map
    }
    pub fn begin_search(&mut self) {
        self.restore_filter = self.filter.clone();
        self.no_match = false;
    }

    pub fn search(&mut self, key: KeyEvent, view: &View<'_>) -> SearchAction {
        match key.code {
            KeyCode::Esc => {
                self.filter = self.restore_filter.clone();
                self.no_match = false;
                SearchAction::Cancel
            }
            KeyCode::Enter => {
                if self.filter.is_empty() {
                    return SearchAction::Accept(Some(view.current_file));
                }
                let file = self.first_file(view);
                self.no_match = file.is_none();
                if file.is_some() {
                    self.restore_filter = self.filter.clone();
                }
                SearchAction::Accept(file)
            }
            KeyCode::Backspace => {
                self.filter.pop();
                self.no_match = false;
                SearchAction::None
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.filter.clear();
                self.no_match = false;
                SearchAction::None
            }
            KeyCode::Char(character)
                if !key.modifiers.intersects(
                    KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER,
                ) =>
            {
                self.filter.push(character);
                self.no_match = false;
                SearchAction::None
            }
            _ => SearchAction::None,
        }
    }
    fn entries(&self, view: &View<'_>) -> Vec<Entry> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        for (file_index, file) in view.files.iter().enumerate() {
            if !self.filter.is_empty() && !fuzzy(&self.filter, &file.path) {
                continue;
            }
            let parts: Vec<_> = file.path.split('/').collect();
            for depth in 0..parts.len().saturating_sub(1) {
                let key = parts[..=depth].join("/");
                if seen.insert(key) {
                    out.push(Entry {
                        label: format!("▰  {}", parts[depth]),
                        depth,
                        target: None,
                    });
                }
            }
            out.push(Entry {
                label: parts.last().unwrap_or(&file.path.as_str()).to_string(),
                depth: parts.len().saturating_sub(1),
                target: Some(Target::File(file_index)),
            });
            for (note, comment) in view
                .comments
                .iter()
                .enumerate()
                .filter(|(_, comment)| comment.path == file.path)
            {
                let text = comment.first_text().replace('\n', " ");
                let mut short = text.chars().take(25).collect::<String>();
                if text.chars().count() > 25 {
                    short.push('…');
                }
                out.push(Entry {
                    label: short,
                    depth: parts.len(),
                    target: Some(Target::Comment {
                        file: file_index,
                        note,
                    }),
                });
            }
        }
        out
    }

    pub fn targets(&self, view: &View<'_>) -> Vec<Target> {
        self.entries(view)
            .into_iter()
            .filter_map(|entry| entry.target)
            .collect()
    }

    pub fn first_file(&self, view: &View<'_>) -> Option<usize> {
        self.targets(view)
            .into_iter()
            .find_map(|target| match target {
                Target::File(file) => Some(file),
                Target::Comment { .. } => None,
            })
    }

    pub fn navigate(&mut self, key: KeyEvent, view: &View<'_>) -> Option<Target> {
        match key.code {
            KeyCode::Char('h') | KeyCode::Left => {
                self.scroll_x = self.scroll_x.saturating_sub(2);
                return None;
            }
            KeyCode::Char('l') | KeyCode::Right => {
                self.scroll_x = self.scroll_x.saturating_add(2);
                return None;
            }
            KeyCode::Char('j') | KeyCode::Down | KeyCode::Char('k') | KeyCode::Up => {}
            _ => return None,
        }
        let targets = self.targets(view);
        if targets.is_empty() {
            return None;
        }
        let current = self.selection.unwrap_or(Target::File(view.current_file));
        let index = targets
            .iter()
            .position(|target| *target == current)
            .unwrap_or(0);
        let delta = if matches!(key.code, KeyCode::Char('j') | KeyCode::Down) {
            1
        } else {
            -1
        };
        let next = (index as isize + delta).clamp(0, targets.len() as isize - 1) as usize;
        let target = targets[next];
        self.selection = Some(target);
        self.follow_selection = true;
        Some(target)
    }

    pub fn draw(&mut self, frame: &mut Frame, area: Rect, view: &View<'_>) {
        self.area = area;
        let entries = self.entries(view);
        let content_width = entries
            .iter()
            .map(|entry| {
                entry.depth * 2
                    + match entry.target {
                        Some(Target::File(_)) => 5 + UnicodeWidthStr::width(entry.label.as_str()),
                        Some(Target::Comment { .. }) => {
                            3 + UnicodeWidthStr::width(entry.label.as_str())
                        }
                        None => UnicodeWidthStr::width(entry.label.as_str()),
                    }
            })
            .max()
            .unwrap_or(0);
        let viewport_width = area.width.saturating_sub(1) as usize;
        self.scroll_x = self
            .scroll_x
            .min(content_width.saturating_sub(viewport_width));
        let active = self.selection.unwrap_or(Target::File(view.current_file));
        let active_row = entries
            .iter()
            .position(|entry| entry.target == Some(active))
            .unwrap_or(0);
        let height = area.height as usize;
        self.scroll_y = self.scroll_y.min(entries.len().saturating_sub(height));
        if self.follow_selection {
            if active_row < self.scroll_y {
                self.scroll_y = active_row;
            } else if active_row >= self.scroll_y.saturating_add(height) {
                self.scroll_y = active_row.saturating_sub(height.saturating_sub(1));
            }
            self.follow_selection = false;
        }
        let visible = entries
            .into_iter()
            .skip(self.scroll_y)
            .take(height)
            .collect::<Vec<_>>();
        self.row_map = visible.iter().map(|entry| entry.target).collect();
        let items = visible
            .into_iter()
            .map(|entry| {
                let indent = "  ".repeat(entry.depth);
                match entry.target {
                    Some(Target::File(file_index)) => {
                        let file = &view.files[file_index];
                        let viewed = view.viewed_files.contains(&file_index);
                        let mut spans = vec![
                            Span::raw(indent),
                            Span::styled(
                                if viewed { "✓ " } else { "  " },
                                Style::default().fg(MUTED),
                            ),
                        ];
                        spans.extend(file_status_spans(file.status));
                        spans.push(Span::raw(" "));
                        spans.push(Span::styled(
                            entry.label,
                            Style::default().fg(if viewed && file_index != view.current_file {
                                MUTED
                            } else {
                                TEXT
                            }),
                        ));
                        ListItem::new(Line::from(crop_spans(spans, self.scroll_x, viewport_width)))
                            .style(if file_index == view.current_file {
                                Style::default().bg(SELECT_BG)
                            } else {
                                Style::default()
                            })
                    }
                    Some(Target::Comment { file, note }) => {
                        let target = Target::Comment { file, note };
                        let selected = if view.focused {
                            self.selection == Some(target)
                        } else {
                            file == view.current_file && view.active_comment == Some(note)
                        };
                        ListItem::new(Line::from(crop_spans(
                            vec![
                                Span::raw(indent),
                                Span::styled("└─ ", Style::default().fg(BORDER)),
                                Span::styled(
                                    entry.label,
                                    Style::default().fg(if selected { TEXT } else { MUTED }),
                                ),
                            ],
                            self.scroll_x,
                            viewport_width,
                        )))
                        .style(if selected {
                            Style::default().bg(SELECT_BG)
                        } else {
                            Style::default()
                        })
                    }
                    None => ListItem::new(Line::from(crop_spans(
                        vec![
                            Span::raw(indent),
                            Span::styled(entry.label, Style::default().fg(MUTED)),
                        ],
                        self.scroll_x,
                        viewport_width,
                    ))),
                }
            })
            .collect::<Vec<_>>();
        frame.render_widget(
            List::new(items)
                .block(Block::default().borders(Borders::RIGHT).border_style(
                    Style::default().fg(if view.focused { super::BLUE } else { BORDER }),
                ))
                .style(Style::default().bg(SURFACE)),
            area,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::parse_unified_diff;

    #[test]
    fn tree_owns_filtering_and_exposes_only_selectable_targets() {
        let files = parse_unified_diff(
            "diff --git a/src/a.rs b/src/a.rs\n--- a/src/a.rs\n+++ b/src/a.rs\n@@ -1 +1 @@\n-a\n+b\ndiff --git a/docs/b.md b/docs/b.md\n--- a/docs/b.md\n+++ b/docs/b.md\n@@ -1 +1 @@\n-a\n+b\n",
        );
        let viewed = HashSet::new();
        let mut tree = FileTree {
            filter: "sr".into(),
            ..FileTree::default()
        };
        let view = View {
            files: &files,
            comments: &[],
            viewed_files: &viewed,
            current_file: 0,
            active_comment: None,
            focused: true,
        };
        assert_eq!(tree.targets(&view), vec![Target::File(0)]);
        tree.filter = "docs".into();
        assert_eq!(tree.first_file(&view), Some(1));
    }

    #[test]
    fn navigation_is_owned_by_the_tree_and_returns_a_selection_action() {
        use crossterm::event::KeyModifiers;
        let files = parse_unified_diff(
            "diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -1 +1 @@\n-a\n+b\ndiff --git a/b b/b\n--- a/b\n+++ b/b\n@@ -1 +1 @@\n-a\n+b\n",
        );
        let viewed = HashSet::new();
        let mut tree = FileTree::default();
        let view = View {
            files: &files,
            comments: &[],
            viewed_files: &viewed,
            current_file: 0,
            active_comment: None,
            focused: true,
        };
        let target = tree.navigate(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), &view);
        assert_eq!(target, Some(Target::File(1)));
        assert_eq!(tree.selection, target);
        assert!(tree.follow_selection);
    }

    #[test]
    fn search_owns_draft_restore_and_acceptance() {
        use crossterm::event::KeyModifiers;
        let files = parse_unified_diff(
            "diff --git a/first b/first\n--- a/first\n+++ b/first\n@@ -1 +1 @@\n-a\n+b\ndiff --git a/second b/second\n--- a/second\n+++ b/second\n@@ -1 +1 @@\n-a\n+b\n",
        );
        let viewed = HashSet::new();
        let mut tree = FileTree {
            filter: "first".into(),
            ..FileTree::default()
        };
        tree.begin_search();
        let view = View {
            files: &files,
            comments: &[],
            viewed_files: &viewed,
            current_file: 0,
            active_comment: None,
            focused: true,
        };
        tree.search(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE), &view);
        assert_eq!(
            tree.search(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), &view),
            SearchAction::Cancel
        );
        assert_eq!(tree.filter, "first");
    }
}
