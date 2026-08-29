use super::{
    App, BG, BatchState, Focus, NEXT_BATCH_KEY, Outcome, PREVIOUS_BATCH_KEY,
    command::{Command, EditorTarget, Effect, RevisionOrigin},
    comment_editor::{Action as EditorAction, CommentEditor},
    diff_pane::{FileViewAction, KeyAction as DiffKeyAction},
    file_tree::{SearchAction, Target as SideTarget, View as FileTreeView},
    help::{EventState as HelpEventState, Help},
    statusline::{Statusline, View as StatuslineView},
    view_helpers::{anchor_position, line_in_comment},
};
use crate::{
    comment::{Comment, format_for_clipboard},
    model::{DiffLine, FileDiff, LineKind, render_unified_diff},
};
use crossterm::event::{Event, KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::Style,
    widgets::Block,
};
use std::time::{Duration, Instant};
impl App {
    pub fn new(files: Vec<FileDiff>, comments: Vec<Comment>) -> Self {
        let mut state = BatchState::new(1, files, RevisionOrigin::Observed);
        state.session.comments = comments;
        Self {
            session: state.session,
            diff_pane: state.diff_pane,
            focus: Focus::Diff,
            search_return_focus: Focus::Diff,
            comment_editor: CommentEditor::default(),
            file_tree: state.file_tree,
            help: Help::default(),
            statusline: Statusline::default(),
            batch_number: 1,
            revision_origin: RevisionOrigin::Observed,
            batch_states: vec![None],
            active_batch: 0,
            watching: false,
        }
    }

    pub fn new_watching(number: u64, files: Vec<FileDiff>) -> Self {
        let mut app = Self::new(files, Vec::new());
        app.batch_number = number;
        app.watching = true;
        app
    }

    pub fn new_watching_context(files: Vec<FileDiff>) -> Self {
        let mut app = Self::new(files, Vec::new());
        app.revision_origin = RevisionOrigin::Context;
        app.batch_number = 0;
        app.watching = true;
        app
    }

    pub fn update(&mut self, command: Command) -> Effect {
        match command {
            Command::Key(key) => match self.key(key) {
                Outcome::Continue => Effect::None,
                Outcome::Finish => Effect::Quit,
                Outcome::ResetWatch => Effect::ResetWatch,
                Outcome::Yank(text) => Effect::Copy(text),
                Outcome::LoadFileView(file) => Effect::RequestFileView(file),
                Outcome::OpenEditor(target) => Effect::OpenEditor(target),
            },
            Command::Mouse(mouse) => {
                self.mouse(mouse);
                Effect::None
            }
            Command::FileViewLoaded { file, lines } => {
                self.finish_file_view_load(file, lines);
                Effect::None
            }
            Command::BatchReceived {
                number,
                files,
                origin,
            } => {
                self.push_batch(number, files, origin);
                Effect::None
            }
            Command::WatchStopped => {
                self.notice("watcher stopped");
                Effect::None
            }
            Command::WatchError(error) => {
                self.notice(format!("watcher: {error}"));
                Effect::None
            }
        }
    }

    #[cfg(test)]
    pub fn set_file_views(&mut self, views: Vec<Option<Vec<DiffLine>>>) {
        if views.len() == self.session.files.len() {
            self.diff_pane.file_views = views;
            self.diff_pane.file_views_loaded.fill(true);
        }
    }

    pub(super) fn finish_file_view_load(&mut self, file: usize, lines: Option<Vec<DiffLine>>) {
        let action = self
            .diff_pane
            .finish_file_view_load(file, lines, &self.session.files);
        self.apply_file_view_action(action);
    }

    pub fn notice(&mut self, message: impl Into<String>) {
        self.statusline.notice(message);
    }

    pub(super) fn current(&self) -> &FileDiff {
        self.diff_pane.current(&self.session.files)
    }

    pub(super) fn selected_bounds(&self) -> (usize, usize) {
        self.diff_pane.selected_bounds()
    }
    pub(super) fn comment_at(&self, p: usize) -> Option<&Comment> {
        let path = &self.current().path;
        self.session
            .comments
            .iter()
            .find(|n| n.path == *path && anchor_position(self.current(), n) == Some(p))
            .or_else(|| {
                self.session
                    .comments
                    .iter()
                    .find(|n| n.path == *path && line_in_comment(&self.current().lines[p], n))
            })
    }
    pub(super) fn anchored_comment_at(&self, p: usize) -> Option<&Comment> {
        let path = &self.current().path;
        self.session
            .comments
            .iter()
            .find(|n| n.path == *path && anchor_position(self.current(), n) == Some(p))
    }
    pub fn draw(&mut self, frame: &mut Frame) {
        let root = frame.area();
        frame.render_widget(Block::default().style(Style::default().bg(BG)), root);
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(8), Constraint::Length(1)])
            .split(root);
        let body = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(36), Constraint::Min(50)])
            .split(rows[0]);
        let active_comment =
            self.session
                .comments
                .iter()
                .enumerate()
                .find_map(|(comment_index, comment)| {
                    (comment.path == self.current().path
                        && anchor_position(self.current(), comment) == Some(self.diff_pane.cursor))
                    .then_some(comment_index)
                });
        self.file_tree.draw(
            frame,
            body[0],
            &FileTreeView {
                files: &self.session.files,
                comments: &self.session.comments,
                reviewed_files: &self.session.reviewed_files,
                current_file: self.diff_pane.file,
                active_comment,
                focused: matches!(self.focus, Focus::Files | Focus::Filter),
            },
        );
        self.diff_pane.draw(
            frame,
            body[1],
            &self.session,
            &self.comment_editor,
            self.focus,
        );
        self.statusline.draw(
            frame,
            rows[1],
            self.focus,
            &StatuslineView {
                session: &self.session,
                pane: &self.diff_pane,
                tree: &self.file_tree,
                revision: self.watching.then_some((
                    self.active_batch + 1,
                    self.batch_states.len(),
                    self.batch_number,
                    self.revision_origin.clone(),
                )),
            },
        );
        self.help.draw(frame, root);
    }

    pub(super) fn move_cursor(&mut self, d: isize) {
        self.diff_pane.move_cursor(d, &self.session.files);
    }
    pub(super) fn switch_file(&mut self, i: usize) {
        if self.diff_pane.switch_file(i, &self.session.files) {
            self.notice("full file unavailable");
        }
    }

    pub(super) fn select_side_target(&mut self, target: SideTarget, keep_sidebar_focus: bool) {
        match target {
            SideTarget::File(file) => self.switch_file(file),
            SideTarget::Comment { file, comment } => {
                if self.diff_pane.file_view {
                    self.diff_pane.file_view_cursors[self.diff_pane.file] = self.diff_pane.cursor;
                    self.diff_pane.file_view = false;
                    self.diff_pane.cursor = self.diff_pane.file_cursors[self.diff_pane.file];
                }
                self.switch_file(file);
                if let Some(position) = self
                    .session
                    .comments
                    .get(comment)
                    .and_then(|annotation| anchor_position(self.current(), annotation))
                {
                    self.diff_pane.cursor = position;
                }
            }
        }
        self.focus = if keep_sidebar_focus {
            Focus::Files
        } else {
            Focus::Diff
        };
        self.file_tree
            .select(keep_sidebar_focus.then_some(target), keep_sidebar_focus);
    }

    pub(super) fn navigate_file_tree(&mut self, key: KeyEvent) {
        let active_comment =
            self.session
                .comments
                .iter()
                .enumerate()
                .find_map(|(comment_index, comment)| {
                    (comment.path == self.session.files[self.diff_pane.file].path
                        && anchor_position(&self.session.files[self.diff_pane.file], comment)
                            == Some(self.diff_pane.cursor))
                    .then_some(comment_index)
                });
        let view = FileTreeView {
            files: &self.session.files,
            comments: &self.session.comments,
            reviewed_files: &self.session.reviewed_files,
            current_file: self.diff_pane.file,
            active_comment,
            focused: true,
        };
        if let Some(target) = self.file_tree.navigate(key, &view) {
            self.select_side_target(target, true);
        }
    }

    pub(super) fn begin_search(&mut self) {
        self.search_return_focus = self.focus;
        self.file_tree.begin_search();
        self.focus = Focus::Filter;
    }

    pub(super) fn toggle_panel_focus(&mut self) {
        if self.focus == Focus::Files {
            self.file_tree.select(None, false);
            self.focus = Focus::Diff;
            return;
        }
        let selected_comment = (!self.diff_pane.file_view)
            .then(|| {
                self.session
                    .comments
                    .iter()
                    .enumerate()
                    .find_map(|(comment_index, comment)| {
                        (comment.path == self.current().path
                            && anchor_position(self.current(), comment)
                                == Some(self.diff_pane.cursor))
                        .then_some(SideTarget::Comment {
                            file: self.diff_pane.file,
                            comment: comment_index,
                        })
                    })
            })
            .flatten();
        self.file_tree.select(
            Some(selected_comment.unwrap_or(SideTarget::File(self.diff_pane.file))),
            true,
        );
        self.focus = Focus::Files;
    }

    pub(super) fn key(&mut self, k: KeyEvent) -> Outcome {
        if self.help.event(&Event::Key(k)) == HelpEventState::Consumed {
            return Outcome::Continue;
        }
        if self.focus == Focus::Editor {
            return self.editor_key(k);
        }
        if self.focus == Focus::Filter {
            return self.filter_key(k);
        }
        if self.watching && matches!(self.focus, Focus::Files | Focus::Diff) {
            match k.code {
                KeyCode::Char('R') => return Outcome::ResetWatch,
                KeyCode::Char(PREVIOUS_BATCH_KEY) => {
                    self.switch_batch(self.active_batch.saturating_sub(1));
                    return Outcome::Continue;
                }
                KeyCode::Char(NEXT_BATCH_KEY) => {
                    self.switch_batch(
                        (self.active_batch + 1).min(self.batch_states.len().saturating_sub(1)),
                    );
                    return Outcome::Continue;
                }
                _ => {}
            }
        }
        if cfg!(test) && self.focus == Focus::Diff && k.code == KeyCode::Char('o') {
            return match self
                .diff_pane
                .request_or_toggle_file_view(&self.session.files)
            {
                FileViewAction::Request(file) => Outcome::LoadFileView(file),
                action => {
                    self.apply_file_view_action(action);
                    Outcome::Continue
                }
            };
        }
        if self.focus == Focus::Files
            && matches!(
                k.code,
                KeyCode::Char('j')
                    | KeyCode::Down
                    | KeyCode::Char('k')
                    | KeyCode::Up
                    | KeyCode::Char('h')
                    | KeyCode::Left
                    | KeyCode::Char('l')
                    | KeyCode::Right
            )
        {
            self.navigate_file_tree(k);
            return Outcome::Continue;
        }
        match self
            .diff_pane
            .key(k, &self.session.files, self.focus == Focus::Diff)
        {
            DiffKeyAction::Copy(text) => {
                let lines = text.lines().count();
                self.notice(format!(
                    "yanked {lines} line{}",
                    if lines == 1 { "" } else { "s" }
                ));
                return Outcome::Yank(text);
            }
            DiffKeyAction::Consumed => return Outcome::Continue,
            DiffKeyAction::Ignored => {}
        }
        match k.code {
            KeyCode::Char('S') => {
                if let Some(handoff) = self.copy_human_handoff() {
                    return Outcome::Yank(handoff);
                }
            }
            KeyCode::Char('Y') => {
                if let Some(comments) = self.copy_comments() {
                    return Outcome::Yank(comments);
                }
            }
            KeyCode::Char('e') => {
                return Outcome::OpenEditor(EditorTarget {
                    path: self.current().path.clone(),
                    line: None,
                    column: None,
                    capture_changes: false,
                });
            }
            KeyCode::Char('s') if !self.diff_pane.file_view => {
                let line = &self.current().lines[self.diff_pane.cursor];
                if !matches!(line.kind, LineKind::Meta | LineKind::Hunk) {
                    return Outcome::OpenEditor(EditorTarget {
                        path: self.current().path.clone(),
                        line: line.new.or(line.old),
                        column: Some(first_code_column(&line.text)),
                        capture_changes: true,
                    });
                }
            }
            KeyCode::Char(' ') => self.toggle_file_reviewed(),
            KeyCode::Enter if self.diff_pane.visual_mode.is_none() && !self.diff_pane.file_view => {
                self.open_editor()
            }
            KeyCode::Char('d') if !self.diff_pane.file_view => self.delete_comment_at_cursor(),
            KeyCode::Char('u') if !self.diff_pane.file_view => self.undo_delete_comment(),
            KeyCode::Char(']') if !self.diff_pane.file_view => self.jump_comment(true),
            KeyCode::Char('[') if !self.diff_pane.file_view => self.jump_comment(false),
            KeyCode::Char('/') => {
                self.begin_search();
            }
            KeyCode::Char('-') | KeyCode::Tab => self.toggle_panel_focus(),
            KeyCode::Char('?') => self.help.open(),
            KeyCode::Char('q') => return Outcome::Finish,
            _ => {}
        }
        Outcome::Continue
    }

    fn push_batch(&mut self, number: u64, files: Vec<FileDiff>, origin: RevisionOrigin) {
        let was_latest = self.active_batch + 1 == self.batch_states.len();
        self.batch_states
            .push(Some(BatchState::new(number, files, origin)));
        if was_latest {
            self.switch_batch(self.batch_states.len() - 1);
        } else {
            let newer = self.batch_states.len() - self.active_batch - 1;
            self.notice(format!("{newer} newer revision(s)"));
        }
    }

    fn switch_batch(&mut self, target: usize) {
        if target == self.active_batch || target >= self.batch_states.len() {
            return;
        }
        let Some(target_state) = self.batch_states[target].take() else {
            return;
        };
        let BatchState {
            number,
            origin,
            session,
            diff_pane,
            file_tree,
        } = target_state;
        let previous = BatchState {
            number: std::mem::replace(&mut self.batch_number, number),
            origin: std::mem::replace(&mut self.revision_origin, origin),
            session: std::mem::replace(&mut self.session, session),
            diff_pane: std::mem::replace(&mut self.diff_pane, diff_pane),
            file_tree: std::mem::replace(&mut self.file_tree, file_tree),
        };
        self.batch_states[self.active_batch] = Some(previous);
        self.active_batch = target;
        self.focus = Focus::Diff;
        self.comment_editor.close();
    }

    pub(super) fn toggle_file_reviewed(&mut self) {
        if self.session.reviewed_files.remove(&self.diff_pane.file) {
            self.notice("file reopened for review");
            return;
        }
        self.session.reviewed_files.insert(self.diff_pane.file);
        self.notice("file reviewed");
        let next = self.session.next_unreviewed_file(self.diff_pane.file);
        if let Some(next) = next {
            self.switch_file(next);
            if self.focus == Focus::Files {
                self.file_tree.select(Some(SideTarget::File(next)), true);
            }
        }
    }

    fn apply_file_view_action(&mut self, action: FileViewAction) {
        if let FileViewAction::Notice(message) = action {
            self.notice(message);
        }
    }

    pub(super) fn copy_comments(&mut self) -> Option<String> {
        let comments = self.comments_for_clipboard();
        if comments.is_empty() {
            self.notice("no comments to copy");
            return None;
        }

        let output = format_for_clipboard(&comments);
        self.notice(format!("copied {} comments", comments.len()));
        Some(output)
    }

    fn copy_human_handoff(&mut self) -> Option<String> {
        if !self.watching {
            self.notice("human handoff is available in watch mode");
            return None;
        }
        if !self.revision_origin.is_human_edit() {
            self.notice("current revision is not human");
            return None;
        }
        let author = self.revision_origin.as_str().to_owned();

        let files = self
            .session
            .files
            .iter()
            .map(|file| format!("- {}", file.path))
            .collect::<Vec<_>>()
            .join("\n");
        let comments = self.session.comments_for_unreviewed_files();
        let comments = (!comments.is_empty())
            .then(|| format!("\n\nReview comments:\n{}", format_for_clipboard(&comments)));
        let handoff = format!(
            "{author} revision #{}\n\nChanged files:\n{files}\n\nDiff:\n```diff\n{}```{}\n\nContinue from the current workspace and treat this revision as pair-programming input from {author}.",
            self.batch_number,
            render_unified_diff(&self.session.files),
            comments.unwrap_or_default(),
        );
        self.notice(format!("copied {author} revision #{}", self.batch_number));
        Some(handoff)
    }

    fn comments_for_clipboard(&self) -> Vec<Comment> {
        if !self.watching {
            return self.session.comments_for_unreviewed_files();
        }

        self.batch_states
            .iter()
            .enumerate()
            .flat_map(|(index, state)| {
                if index == self.active_batch {
                    self.session.comments_for_unreviewed_files()
                } else {
                    state
                        .as_ref()
                        .map(|state| state.session.comments_for_unreviewed_files())
                        .unwrap_or_default()
                }
            })
            .collect()
    }

    pub(super) fn filter_key(&mut self, k: KeyEvent) -> Outcome {
        let active_comment =
            self.session
                .comments
                .iter()
                .enumerate()
                .find_map(|(comment_index, comment)| {
                    (comment.path == self.session.files[self.diff_pane.file].path
                        && anchor_position(&self.session.files[self.diff_pane.file], comment)
                            == Some(self.diff_pane.cursor))
                    .then_some(comment_index)
                });
        let view = FileTreeView {
            files: &self.session.files,
            comments: &self.session.comments,
            reviewed_files: &self.session.reviewed_files,
            current_file: self.diff_pane.file,
            active_comment,
            focused: true,
        };
        match self.file_tree.search(k, &view) {
            SearchAction::Cancel => {
                self.focus = self.search_return_focus;
            }
            SearchAction::Accept(file) => {
                if let Some(file) = file {
                    self.select_side_target(SideTarget::File(file), true)
                }
            }
            SearchAction::None => {}
        }
        Outcome::Continue
    }
    pub(super) fn editor_key(&mut self, k: KeyEvent) -> Outcome {
        match self.comment_editor.event(k) {
            EditorAction::Cancel => {
                self.focus = Focus::Diff;
                self.comment_editor.close();
            }
            EditorAction::Save => self.save_editor(),
            EditorAction::None => {}
        }
        Outcome::Continue
    }

    pub(super) fn open_editor(&mut self) {
        if matches!(
            self.current().lines[self.diff_pane.cursor].kind,
            LineKind::Meta
        ) {
            return;
        }
        let existing = self.anchored_comment_at(self.diff_pane.cursor).cloned();
        let text = existing
            .as_ref()
            .map_or(String::new(), |comment| comment.first_text().to_owned());
        let range_bottom = self.selected_bounds().1;
        let anchor = existing
            .as_ref()
            .and_then(|n| anchor_position(self.current(), n))
            .unwrap_or(range_bottom);
        self.comment_editor
            .open(text, anchor, existing.as_ref().map(|comment| comment.key()));
        self.focus = Focus::Editor
    }
    pub(super) fn save_editor(&mut self) {
        let range = self.selected_bounds();
        let anchor = self.comment_editor.anchor.unwrap_or(range.1);
        let editing = self.comment_editor.editing_key.is_some();
        let has_text = !self.comment_editor.text.trim().is_empty();
        if !self.session.save_comment(
            self.diff_pane.file,
            range,
            anchor,
            self.comment_editor.editing_key.take(),
            &self.comment_editor.text,
        ) {
            return;
        }
        self.diff_pane.range_anchor = None;
        self.focus = Focus::Diff;
        self.comment_editor.close();
        self.notice(if !has_text {
            "empty comment discarded"
        } else if editing {
            "comment updated"
        } else {
            "comment saved · Space when file is ready"
        });
    }
    pub(super) fn delete_comment_at_cursor(&mut self) {
        if let Some(key) = self
            .comment_at(self.diff_pane.cursor)
            .map(|comment| comment.key())
            && self.session.delete_comment(&key)
        {
            if self.focus == Focus::Files {
                self.file_tree
                    .select(Some(SideTarget::File(self.diff_pane.file)), false);
            }
            self.notice("comment deleted · u undo");
        }
    }

    pub(super) fn undo_delete_comment(&mut self) {
        if let Some(restored) = self.session.restore_comment() {
            if self.focus == Focus::Files {
                self.file_tree.select(
                    Some(SideTarget::Comment {
                        file: self.diff_pane.file,
                        comment: restored,
                    }),
                    false,
                );
            }
            self.notice("deleted comment restored");
        }
    }
    pub(super) fn jump_comment(&mut self, next: bool) {
        let mut p = self
            .session
            .comments
            .iter()
            .filter(|n| n.path == self.current().path)
            .filter_map(|n| anchor_position(self.current(), n))
            .collect::<Vec<_>>();
        p.sort_unstable();
        if p.is_empty() {
            return;
        }
        self.diff_pane.cursor = if next {
            p.iter()
                .copied()
                .find(|x| *x > self.diff_pane.cursor)
                .unwrap_or(p[0])
        } else {
            p.iter()
                .rev()
                .copied()
                .find(|x| *x < self.diff_pane.cursor)
                .unwrap_or(*p.last().unwrap())
        }
    }

    pub(super) fn mouse(&mut self, m: MouseEvent) {
        if self.help.is_open() {
            return;
        }
        match m.kind {
            MouseEventKind::ScrollDown => {
                if self.file_tree.contains(m.column, m.row) {
                    self.file_tree.scroll_vertical(3);
                } else {
                    self.move_cursor(3)
                }
            }
            MouseEventKind::ScrollUp => {
                if self.file_tree.contains(m.column, m.row) {
                    self.file_tree.scroll_vertical(-3);
                } else {
                    self.move_cursor(-3)
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if self.diff_pane.area.contains((m.column, m.row).into()) {
                    let row = m.row.saturating_sub(self.diff_pane.area.y) as usize;
                    if let Some(p) = self.diff_pane.row_map.get(row).copied().flatten() {
                        self.diff_pane.cursor = p;
                        self.focus = Focus::Diff;
                        let now = Instant::now();
                        if !self.diff_pane.file_view
                            && self.diff_pane.last_click.is_some_and(|(t, x)| {
                                x == p && now.duration_since(t) < Duration::from_millis(450)
                            })
                        {
                            self.open_editor()
                        }
                        self.diff_pane.last_click = Some((now, p));
                    }
                } else if self.file_tree.contains(m.column, m.row) {
                    self.focus = Focus::Files;
                    if let Some(target) = self.file_tree.target_at(m.row) {
                        self.select_side_target(target, true);
                    }
                }
            }
            _ => {}
        }
    }
}

fn first_code_column(code: &str) -> u32 {
    code.chars()
        .take_while(|character| character.is_whitespace())
        .count() as u32
        + 1
}
