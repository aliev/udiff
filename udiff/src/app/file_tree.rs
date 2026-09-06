use super::{render::file_status_spans, search::fuzzy, view_helpers::plural};
use crate::theme::theme;
use crate::{comment::Comment, model::FileDiff};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Position, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use std::collections::HashSet;
use tui_tree_widget::{Tree, TreeItem, TreeState};
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Target {
    File(usize),
    Comment { file: usize, comment: usize },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchAction {
    None,
    Cancel,
    Accept(Option<usize>),
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum NodeId {
    Folder(String),
    File(usize),
    Comment { file: usize, comment: usize },
}

impl NodeId {
    fn target(&self) -> Option<Target> {
        match *self {
            Self::Folder(_) => None,
            Self::File(file) => Some(Target::File(file)),
            Self::Comment { file, comment } => Some(Target::Comment { file, comment }),
        }
    }
}

#[derive(Default)]
struct Folder {
    name: String,
    path: String,
    folders: Vec<Self>,
    files: Vec<usize>,
}

impl Folder {
    fn insert(&mut self, path: &str, file: usize) {
        let parts = path.split('/').collect::<Vec<_>>();
        let mut folder = self;
        for part in parts.iter().take(parts.len().saturating_sub(1)) {
            let path = if folder.path.is_empty() {
                (*part).to_owned()
            } else {
                format!("{}/{}", folder.path, part)
            };
            let position = folder
                .folders
                .iter()
                .position(|child| child.name == *part)
                .unwrap_or_else(|| {
                    folder.folders.push(Self {
                        name: (*part).to_owned(),
                        path,
                        ..Self::default()
                    });
                    folder.folders.len() - 1
                });
            folder = &mut folder.folders[position];
        }
        folder.files.push(file);
    }

    fn items(&self, view: &View<'_>, depth: usize) -> Vec<TreeItem<'static, NodeId>> {
        let mut items = self
            .folders
            .iter()
            .map(|folder| folder.item(view, depth))
            .collect::<Vec<_>>();
        items.extend(self.files.iter().map(|file| file_item(*file, view, depth)));
        items
    }

    fn item(&self, view: &View<'_>, depth: usize) -> TreeItem<'static, NodeId> {
        TreeItem::new(
            NodeId::Folder(self.path.clone()),
            Line::from(Span::styled(
                format!("{}/", self.name),
                Style::default()
                    .fg(theme().muted)
                    .add_modifier(Modifier::BOLD),
            )),
            self.items(view, depth + 1),
        )
        .expect("folder children have unique identifiers")
    }
}

pub struct FileTree {
    filter: String,
    restore_filter: String,
    no_match: bool,
    area: Rect,
    state: TreeState<NodeId>,
    known_nodes: HashSet<Vec<NodeId>>,
    selection: Option<Target>,
    selection_dirty: bool,
}

impl Default for FileTree {
    fn default() -> Self {
        Self {
            filter: String::new(),
            restore_filter: String::new(),
            no_match: false,
            area: Rect::default(),
            state: TreeState::default(),
            known_nodes: HashSet::new(),
            selection: None,
            selection_dirty: true,
        }
    }
}

pub struct View<'a> {
    pub files: &'a [FileDiff],
    pub comments: &'a [Comment],
    pub reviewed_files: &'a HashSet<usize>,
    pub current_file: usize,
    pub active_comment: Option<usize>,
    pub focused: bool,
    /// Whether the diff pane sits to the right, so the explorer knows if it
    /// should draw a divider and join it into the header rule.
    pub divided: bool,
    /// Columns the explorer occupies, so a label can be cut to fit rather than
    /// to a fixed length.
    pub width: u16,
}

impl FileTree {
    pub fn filter(&self) -> &str {
        &self.filter
    }

    pub fn width(&self) -> u16 {
        self.area.width
    }

    pub fn no_match(&self) -> bool {
        self.no_match
    }

    #[cfg(test)]
    pub fn selection(&self) -> Option<Target> {
        self.selection
    }

    pub fn select(&mut self, target: Option<Target>) {
        self.selection = target;
        self.selection_dirty = true;
    }

    pub fn contains(&self, column: u16, row: u16) -> bool {
        self.area.contains((column, row).into())
    }

    pub fn scroll_vertical(&mut self, delta: isize) {
        if delta < 0 {
            self.state.scroll_up(delta.unsigned_abs());
        } else {
            self.state.scroll_down(delta as usize);
        }
    }

    pub fn click(&mut self, column: u16, row: u16) -> Option<Target> {
        if !self.state.click_at(Position::new(column, row)) {
            return None;
        }
        self.selection = self.selected_target();
        self.selection
    }

    #[cfg(test)]
    pub fn set_filter(&mut self, filter: impl Into<String>) {
        self.filter = filter.into();
    }

    #[cfg(test)]
    pub fn scroll_y(&self) -> usize {
        self.state.get_offset()
    }

    pub fn begin_search(&mut self) {
        self.restore_filter.clone_from(&self.filter);
        self.no_match = false;
    }

    pub fn search(&mut self, key: KeyEvent, view: &View<'_>) -> SearchAction {
        match key.code {
            KeyCode::Esc => {
                self.filter.clone_from(&self.restore_filter);
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
                    self.restore_filter.clone_from(&self.filter);
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

    pub fn first_file(&self, view: &View<'_>) -> Option<usize> {
        view.files
            .iter()
            .position(|file| self.filter.is_empty() || fuzzy(&self.filter, &file.path))
    }

    pub fn navigate(&mut self, key: KeyEvent) -> Option<Target> {
        let changed = match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.state.key_down(),
            KeyCode::Char('k') | KeyCode::Up => self.state.key_up(),
            KeyCode::Char('h') | KeyCode::Left => self.state.key_left(),
            KeyCode::Char('l') | KeyCode::Right => self.state.key_right(),
            _ => return None,
        };
        if !changed {
            return None;
        }
        self.selection = self.selected_target();
        self.selection
    }

    pub fn draw(&mut self, frame: &mut Frame, area: Rect, view: &View<'_>) {
        self.area = Rect::default();
        if area.width == 0 || area.height == 0 {
            return;
        }
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(area);
        self.draw_summary(frame, rows[0], view);

        let list_area = rows[1];
        self.area = list_area;
        let items = self.items(view);
        open_new_nodes(
            &items,
            &mut self.state,
            &mut self.known_nodes,
            &mut Vec::new(),
        );
        self.sync_selection(&items, view.current_file);

        let tree = Tree::new(&items)
            .expect("tree roots have unique identifiers")
            .block(
                Block::default()
                    .borders(if view.divided {
                        Borders::RIGHT
                    } else {
                        Borders::NONE
                    })
                    .border_style(Style::default().fg(if view.focused {
                        theme().blue
                    } else {
                        theme().border
                    })),
            )
            .style(Style::default().fg(theme().muted).bg(theme().surface))
            .highlight_style(highlight_style(view.focused))
            .highlight_symbol("▌")
            // Every expander is two cells wide, glyph plus its space, so a
            // folder and a file open the same distance from their label and a
            // leaf still reserves the cell a commented file would use.
            .node_closed_symbol("▸ ")
            .node_open_symbol("▾ ")
            .node_no_children_symbol("  ");
        frame.render_stateful_widget(tree, list_area, &mut self.state);
    }

    fn items(&self, view: &View<'_>) -> Vec<TreeItem<'static, NodeId>> {
        let mut root = Folder::default();
        for (file, diff) in view.files.iter().enumerate() {
            if self.filter.is_empty() || fuzzy(&self.filter, &diff.path) {
                root.insert(&diff.path, file);
            }
        }
        root.items(view, 0)
    }

    fn sync_selection(&mut self, items: &[TreeItem<'_, NodeId>], current_file: usize) {
        if !self.selection_dirty {
            return;
        }
        let selected = self
            .selection
            .or(Some(Target::File(current_file)))
            .and_then(|target| find_target(items, target, &mut Vec::new()))
            .unwrap_or_default();
        self.state.select(selected);
        self.selection_dirty = false;
    }

    fn selected_target(&self) -> Option<Target> {
        self.state.selected().last().and_then(NodeId::target)
    }

    fn draw_summary(&self, frame: &mut Frame, area: Rect, view: &View<'_>) {
        let total = view.files.len();
        let reviewed = view.reviewed_files.len();
        let complete = total > 0 && reviewed == total;
        let comments = plural(view.comments.len(), "comment");
        let inner = usize::from(area.width).saturating_sub(usize::from(view.divided));
        let mut spans = Vec::new();
        if complete {
            spans.push(Span::styled(
                " \u{2713} done",
                Style::default()
                    .fg(theme().green)
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(
                format!(" \u{b7} {comments}"),
                Style::default().fg(theme().muted),
            ));
        } else {
            // Counters first: an empty meter is nearly invisible and would
            // open the row with a void.
            let label = format!(" {reviewed}/{total} \u{b7} {comments}");
            let meter = meter_width(inner, UnicodeWidthStr::width(label.as_str()));
            spans.push(Span::styled(label, Style::default().fg(theme().muted)));
            // An untouched review has nothing to measure, and an all-empty
            // track reads as a stray rule rather than as a meter.
            if meter > 0 && reviewed > 0 {
                spans.push(Span::raw("  "));
                spans.extend(progress_spans(reviewed, total, meter));
            }
        }
        frame.render_widget(
            Paragraph::new(Line::from(spans))
                .block(
                    Block::default()
                        .borders(if view.divided {
                            Borders::RIGHT
                        } else {
                            Borders::NONE
                        })
                        .border_style(Style::default().fg(if view.focused {
                            theme().blue
                        } else {
                            theme().border
                        })),
                )
                .style(Style::default().bg(theme().surface)),
            area,
        );
    }
}

/// The row the tree cursor is on. Only a focused explorer paints a
/// background: an unfocused row used to take the canvas colour, which recedes
/// from the panel in the dark palette but stands out as a bright band in the
/// light one. The `▌` symbol and the bold filename mark the row in both.
fn highlight_style(focused: bool) -> Style {
    if focused {
        theme()
            .selected(Style::default())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    }
}

/// Columns a label at `depth` may spend on its excerpt. Returns `0` when what
/// remains is too short to say anything, in which case the excerpt is dropped
/// rather than cut to a stub.
fn excerpt_budget(width: u16, depth: usize, head_width: usize) -> usize {
    // One cell of highlight gutter, two per level of nesting, two for the
    // expander, and one for the divider on the right.
    let indent = 1 + depth * 2 + 2;
    let free = usize::from(width)
        .saturating_sub(indent + 1)
        .saturating_sub(head_width + 2);
    if free < 4 { 0 } else { free }
}

fn ellipsised(text: &str, budget: usize) -> String {
    if UnicodeWidthStr::width(text) <= budget {
        return text.to_owned();
    }
    let mut out = String::new();
    let mut used = 0;
    for character in text.chars() {
        let step = UnicodeWidthStr::width(character.to_string().as_str());
        if used + step > budget - 1 {
            break;
        }
        out.push(character);
        used += step;
    }
    out.push('\u{2026}');
    out
}

/// Columns left for the review meter once the counters have taken theirs.
/// Returns `0` when what remains is too small to read as a meter.
fn meter_width(inner: usize, label_width: usize) -> usize {
    let free = inner.saturating_sub(label_width + 2).min(10);
    if free < 3 { 0 } else { free }
}

fn progress_spans(reviewed: usize, total: usize, width: usize) -> Vec<Span<'static>> {
    let filled = (reviewed * width).checked_div(total).unwrap_or(0);
    vec![
        Span::styled(
            "\u{2501}".repeat(filled),
            Style::default().fg(theme().green),
        ),
        Span::styled(
            "\u{2500}".repeat(width - filled),
            Style::default().fg(theme().border),
        ),
    ]
}

fn file_item(file: usize, view: &View<'_>, depth: usize) -> TreeItem<'static, NodeId> {
    let diff = &view.files[file];
    let reviewed = view.reviewed_files.contains(&file);
    let current = file == view.current_file;
    let name = diff.path.rsplit('/').next().unwrap_or(&diff.path);
    let comments = view
        .comments
        .iter()
        .enumerate()
        .filter(|(_, comment)| comment.path == diff.path)
        .collect::<Vec<_>>();
    let mut label = vec![Span::styled(
        if reviewed { "✓ " } else { "  " },
        Style::default().fg(if reviewed {
            theme().green
        } else {
            theme().muted
        }),
    )];
    label.extend(file_status_spans(diff.status));
    label.push(Span::styled(
        name.to_owned(),
        Style::default()
            .fg(if reviewed && !current {
                theme().muted
            } else {
                theme().text
            })
            .add_modifier(if current {
                Modifier::BOLD
            } else {
                Modifier::empty()
            }),
    ));
    if !comments.is_empty() {
        label.push(Span::styled(
            format!("  {}", comments.len()),
            Style::default().fg(theme().comment),
        ));
    }

    let children = comments
        .into_iter()
        .enumerate()
        .map(|(number, (comment, item))| comment_item(file, comment, number, item, view, depth + 1))
        .collect();
    TreeItem::new(NodeId::File(file), Line::from(label), children)
        .expect("comment identifiers are unique")
}

fn comment_item(
    file: usize,
    comment: usize,
    number: usize,
    item: &Comment,
    view: &View<'_>,
    depth: usize,
) -> TreeItem<'static, NodeId> {
    let marker = if item.body.is_suggestion() { "S#" } else { "#" };
    let head = format!("{}  {marker}{}", item.short_location(), number + 1);
    let text = item.first_text().replace('\n', " ");
    let label = match excerpt_budget(view.width, depth, UnicodeWidthStr::width(head.as_str())) {
        0 => head,
        budget => format!("{head}  {}", ellipsised(&text, budget)),
    };
    let selected = file == view.current_file && view.active_comment == Some(comment);
    TreeItem::new_leaf(
        NodeId::Comment { file, comment },
        Line::from(Span::styled(
            label,
            Style::default()
                .fg(if selected {
                    theme().text
                } else {
                    theme().muted
                })
                .bg(if selected {
                    theme().comment_bg
                } else {
                    theme().surface
                }),
        )),
    )
}

fn find_target(
    items: &[TreeItem<'_, NodeId>],
    target: Target,
    parents: &mut Vec<NodeId>,
) -> Option<Vec<NodeId>> {
    for item in items {
        parents.push(item.identifier().clone());
        if item.identifier().target() == Some(target) {
            return Some(parents.clone());
        }
        if let Some(path) = find_target(item.children(), target, parents) {
            return Some(path);
        }
        parents.pop();
    }
    None
}

fn open_new_nodes(
    items: &[TreeItem<'_, NodeId>],
    state: &mut TreeState<NodeId>,
    known_nodes: &mut HashSet<Vec<NodeId>>,
    parents: &mut Vec<NodeId>,
) {
    for item in items {
        parents.push(item.identifier().clone());
        let is_new = known_nodes.insert(parents.clone());
        if is_new && !item.children().is_empty() {
            state.open(parents.clone());
        }
        open_new_nodes(item.children(), state, known_nodes, parents);
        parents.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::parse_unified_diff;
    use ratatui::{Terminal, backend::TestBackend};

    fn view<'a>(files: &'a [FileDiff], reviewed: &'a HashSet<usize>) -> View<'a> {
        View {
            files,
            comments: &[],
            reviewed_files: reviewed,
            current_file: 0,
            active_comment: None,
            focused: true,
            divided: true,
            width: 36,
        }
    }

    #[test]
    fn an_unfocused_tree_row_never_paints_over_the_panel() {
        use crate::theme::{Mode, Palette};
        assert_eq!(highlight_style(false).bg, None);
        assert_eq!(highlight_style(true).bg, Some(theme().select_bg));
        // The canvas colour is only a recess in one palette, never both, which
        // is why an unfocused row must not reach for it.
        let brightness = |colour| match colour {
            ratatui::style::Color::Rgb(r, g, b) => u32::from(r) + u32::from(g) + u32::from(b),
            other => panic!("expected an RGB colour, got {other:?}"),
        };
        let light = Palette::for_mode(Mode::Light);
        let dark = Palette::for_mode(Mode::Dark);
        assert!(brightness(light.bg) > brightness(light.surface));
        assert!(brightness(dark.bg) < brightness(dark.surface));
    }

    #[test]
    fn the_meter_stays_away_until_there_is_progress_to_show() {
        // Nothing reviewed: the counters alone, no all-empty track.
        assert_eq!(meter_width(30, 18), 10);
        let spans = progress_spans(0, 4, 8);
        assert_eq!(spans[0].content.chars().count(), 0);
    }

    #[test]
    fn the_review_meter_shows_progress_in_its_glyphs_not_only_its_colour() {
        let spans = progress_spans(1, 4, 8);
        let filled = spans[0].content.chars().count();
        let empty = spans[1].content.chars().count();
        assert_eq!((filled, empty), (2, 6));
        assert_ne!(
            spans[0].content.chars().next(),
            spans[1].content.chars().next(),
            "a meter drawn in one glyph reads as a plain rule when empty"
        );
    }

    #[test]
    fn filtering_selects_the_first_matching_file() {
        let files = parse_unified_diff(
            "diff --git a/src/a.rs b/src/a.rs\n--- a/src/a.rs\n+++ b/src/a.rs\n@@ -1 +1 @@\n-a\n+b\ndiff --git a/docs/b.md b/docs/b.md\n--- a/docs/b.md\n+++ b/docs/b.md\n@@ -1 +1 @@\n-a\n+b\n",
        );
        let reviewed = HashSet::new();
        let mut tree = FileTree {
            filter: "sr".into(),
            ..FileTree::default()
        };
        assert_eq!(tree.first_file(&view(&files, &reviewed)), Some(0));
        tree.filter = "docs".into();
        assert_eq!(tree.first_file(&view(&files, &reviewed)), Some(1));
    }

    #[test]
    fn widget_renders_nested_paths_without_manual_row_math() {
        let files = parse_unified_diff(
            "diff --git a/src/deep/a.rs b/src/deep/a.rs\n--- a/src/deep/a.rs\n+++ b/src/deep/a.rs\n@@ -1 +1 @@\n-a\n+b\n",
        );
        let reviewed = HashSet::new();
        let mut tree = FileTree::default();
        let mut terminal = Terminal::new(TestBackend::new(24, 8)).unwrap();
        terminal
            .draw(|frame| tree.draw(frame, frame.area(), &view(&files, &reviewed)))
            .unwrap();
        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains("src/"));
        assert!(rendered.contains("deep/"));
        assert!(rendered.contains("a.rs"));
    }

    #[test]
    fn navigation_follows_rendered_tree_order() {
        let files = parse_unified_diff(
            "diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -1 +1 @@\n-a\n+b\ndiff --git a/b b/b\n--- a/b\n+++ b/b\n@@ -1 +1 @@\n-a\n+b\n",
        );
        let reviewed = HashSet::new();
        let mut tree = FileTree::default();
        let mut terminal = Terminal::new(TestBackend::new(24, 8)).unwrap();
        terminal
            .draw(|frame| tree.draw(frame, frame.area(), &view(&files, &reviewed)))
            .unwrap();
        let target = tree.navigate(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(target, Some(Target::File(1)));
    }

    #[test]
    fn mouse_selection_uses_the_rendered_row() {
        let files = parse_unified_diff(
            "diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -1 +1 @@\n-a\n+b\ndiff --git a/b b/b\n--- a/b\n+++ b/b\n@@ -1 +1 @@\n-a\n+b\n",
        );
        let reviewed = HashSet::new();
        let mut tree = FileTree::default();
        let mut terminal = Terminal::new(TestBackend::new(24, 8)).unwrap();
        terminal
            .draw(|frame| tree.draw(frame, frame.area(), &view(&files, &reviewed)))
            .unwrap();

        assert_eq!(tree.click(1, 3), Some(Target::File(1)));
    }

    #[test]
    fn collapsed_folders_stay_collapsed_after_rendering() {
        let files = parse_unified_diff(
            "diff --git a/src/deep/a.rs b/src/deep/a.rs\n--- a/src/deep/a.rs\n+++ b/src/deep/a.rs\n@@ -1 +1 @@\n-a\n+b\n",
        );
        let reviewed = HashSet::new();
        let mut tree = FileTree::default();
        let mut terminal = Terminal::new(TestBackend::new(24, 8)).unwrap();
        terminal
            .draw(|frame| tree.draw(frame, frame.area(), &view(&files, &reviewed)))
            .unwrap();

        tree.navigate(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        tree.navigate(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        terminal
            .draw(|frame| tree.draw(frame, frame.area(), &view(&files, &reviewed)))
            .unwrap();

        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains("deep/"));
        assert!(!rendered.contains("a.rs"));
    }

    #[test]
    fn search_restores_the_previous_filter_on_escape() {
        let files = parse_unified_diff(
            "diff --git a/first b/first\n--- a/first\n+++ b/first\n@@ -1 +1 @@\n-a\n+b\n",
        );
        let reviewed = HashSet::new();
        let mut tree = FileTree {
            filter: "first".into(),
            ..FileTree::default()
        };
        tree.begin_search();
        tree.search(
            KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
            &view(&files, &reviewed),
        );
        assert_eq!(
            tree.search(
                KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
                &view(&files, &reviewed),
            ),
            SearchAction::Cancel
        );
        assert_eq!(tree.filter, "first");
    }
}
