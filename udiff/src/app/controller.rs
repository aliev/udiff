//! Routes terminal input to the component that owns the behavior.

use super::{
    App, Focus, NEXT_BATCH_KEY, PREVIOUS_BATCH_KEY, RevisionState,
    command::{Command, EditorTarget, Effect},
    diff_pane::KeyAction as DiffKeyAction,
    help::EventState as HelpEventState,
};
use crate::{comment::Comment, model::FileDiff};
use crossterm::event::{Event, KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use std::time::{Duration, Instant};

impl App {
    pub fn new(files: Vec<FileDiff>, comments: Vec<Comment>) -> Self {
        let mut state = RevisionState::new(1, files);
        state.session.comments = comments;
        Self {
            session: state.session,
            diff_pane: state.diff_pane,
            focus: Focus::Diff,
            search_return_focus: Focus::Diff,
            comment_editor: Default::default(),
            file_tree: state.file_tree,
            help: Default::default(),
            statusline: Default::default(),
            revision_number: 1,
            revision_states: vec![None],
            active_revision: 0,
            watching: false,
        }
    }

    /// The single boundary for input and external results.
    ///
    /// Components mutate application state; only this method translates their
    /// decisions into effects that the terminal runtime performs.
    pub fn update(&mut self, command: Command) -> Effect {
        match command {
            Command::Key(key) => self.key(key),
            Command::Mouse(mouse) => {
                self.mouse(mouse);
                Effect::None
            }
            Command::RevisionReceived { number, files } => {
                self.push_revision(number, files);
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

    pub fn notice(&mut self, message: impl Into<String>) {
        self.statusline.notice(message);
    }

    pub(super) fn key(&mut self, key: KeyEvent) -> Effect {
        if self.help.event(&Event::Key(key)) == HelpEventState::Consumed {
            return Effect::None;
        }
        if self.focus == Focus::Editor {
            return self.editor_key(key);
        }
        if self.focus == Focus::Filter {
            return self.filter_key(key);
        }

        if self.watching && matches!(self.focus, Focus::Files | Focus::Diff) {
            match key.code {
                KeyCode::Char('R') => return Effect::ResetWatch,
                KeyCode::Char(PREVIOUS_BATCH_KEY) => {
                    self.switch_revision(self.active_revision.saturating_sub(1));
                    return Effect::None;
                }
                KeyCode::Char(NEXT_BATCH_KEY) => {
                    self.switch_revision(
                        (self.active_revision + 1)
                            .min(self.revision_states.len().saturating_sub(1)),
                    );
                    return Effect::None;
                }
                _ => {}
            }
        }

        if self.focus == Focus::Files && is_file_navigation_key(key.code) {
            self.navigate_file_tree(key);
            return Effect::None;
        }

        match self
            .diff_pane
            .key(key, &self.session.files, self.focus == Focus::Diff)
        {
            DiffKeyAction::Copy(text) => {
                let lines = text.lines().count();
                self.notice(format!(
                    "yanked {lines} line{}",
                    if lines == 1 { "" } else { "s" }
                ));
                return Effect::Copy(text);
            }
            DiffKeyAction::Consumed => return Effect::None,
            DiffKeyAction::Ignored => {}
        }

        match key.code {
            KeyCode::Char('Y') => self.copy_comments().map_or(Effect::None, Effect::Copy),
            KeyCode::Char('e') => Effect::OpenEditor(EditorTarget {
                path: self.current().path.clone(),
            }),
            KeyCode::Char('r') if self.focus == Focus::Diff => {
                self.open_suggestion_editor();
                Effect::None
            }
            KeyCode::Char(' ') => {
                self.toggle_file_reviewed();
                Effect::None
            }
            KeyCode::Enter if self.diff_pane.visual_mode.is_none() => {
                self.open_editor();
                Effect::None
            }
            KeyCode::Char('d') => {
                self.delete_comment_at_cursor();
                Effect::None
            }
            KeyCode::Char('u') => {
                self.undo_delete_comment();
                Effect::None
            }
            KeyCode::Char(']') => {
                self.jump_comment(true);
                Effect::None
            }
            KeyCode::Char('[') => {
                self.jump_comment(false);
                Effect::None
            }
            KeyCode::Char('/') => {
                self.begin_search();
                Effect::None
            }
            KeyCode::Char('-') | KeyCode::Tab => {
                self.toggle_panel_focus();
                Effect::None
            }
            KeyCode::Char('?') => {
                self.help.open();
                Effect::None
            }
            KeyCode::Char('q') => Effect::Quit,
            _ => Effect::None,
        }
    }

    fn mouse(&mut self, mouse: MouseEvent) {
        if self.help.is_open() {
            return;
        }
        match mouse.kind {
            MouseEventKind::ScrollDown => {
                if self.file_tree.contains(mouse.column, mouse.row) {
                    self.file_tree.scroll_vertical(3);
                } else {
                    self.move_cursor(3);
                }
            }
            MouseEventKind::ScrollUp => {
                if self.file_tree.contains(mouse.column, mouse.row) {
                    self.file_tree.scroll_vertical(-3);
                } else {
                    self.move_cursor(-3);
                }
            }
            MouseEventKind::Down(MouseButton::Left) => self.left_click(mouse),
            _ => {}
        }
    }

    fn left_click(&mut self, mouse: MouseEvent) {
        if self
            .diff_pane
            .area
            .contains((mouse.column, mouse.row).into())
        {
            let row = mouse.row.saturating_sub(self.diff_pane.area.y) as usize;
            if let Some(position) = self.diff_pane.row_map.get(row).copied().flatten() {
                self.diff_pane.cursor = position;
                self.focus = Focus::Diff;
                let now = Instant::now();
                if self.diff_pane.last_click.is_some_and(|(time, previous)| {
                    previous == position && now.duration_since(time) < Duration::from_millis(450)
                }) {
                    self.open_editor();
                }
                self.diff_pane.last_click = Some((now, position));
            }
        } else if self.file_tree.contains(mouse.column, mouse.row) {
            self.focus = Focus::Files;
            if let Some(target) = self.file_tree.click(mouse.column, mouse.row) {
                self.select_side_target(target, true);
            }
        }
    }
}

fn is_file_navigation_key(key: KeyCode) -> bool {
    matches!(
        key,
        KeyCode::Char('j')
            | KeyCode::Down
            | KeyCode::Char('k')
            | KeyCode::Up
            | KeyCode::Char('h')
            | KeyCode::Left
            | KeyCode::Char('l')
            | KeyCode::Right
    )
}
