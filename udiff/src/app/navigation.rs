//! File selection, sidebar focus, and search coordination.

use super::{
    App, Focus,
    command::Effect,
    file_tree::{SearchAction, Target as SideTarget, View as FileTreeView},
    view_helpers::anchor_position,
};
use crossterm::event::KeyEvent;

impl App {
    pub(super) fn active_comment_index(&self) -> Option<usize> {
        self.session
            .comments
            .iter()
            .enumerate()
            .find_map(|(index, comment)| {
                (comment.path == self.current().path
                    && anchor_position(self.current(), comment) == Some(self.diff_pane.cursor))
                .then_some(index)
            })
    }

    pub(super) fn move_cursor(&mut self, delta: isize) {
        self.diff_pane.move_cursor(delta, &self.session.files);
    }

    pub(super) fn switch_file(&mut self, file: usize) {
        self.diff_pane.switch_file(file, &self.session.files);
    }

    pub(super) fn select_side_target(&mut self, target: SideTarget, keep_sidebar_focus: bool) {
        match target {
            SideTarget::File(file) => self.switch_file(file),
            SideTarget::Comment { file, comment } => {
                self.switch_file(file);
                if let Some(position) = self
                    .session
                    .comments
                    .get(comment)
                    .and_then(|comment| anchor_position(self.current(), comment))
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
        self.file_tree.select(keep_sidebar_focus.then_some(target));
    }

    pub(super) fn navigate_file_tree(&mut self, key: KeyEvent) {
        if let Some(target) = self.file_tree.navigate(key) {
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
            self.file_tree.select(None);
            self.focus = Focus::Diff;
            return;
        }
        let selected_comment = self
            .active_comment_index()
            .map(|comment| SideTarget::Comment {
                file: self.diff_pane.file,
                comment,
            });
        self.file_tree.select(Some(
            selected_comment.unwrap_or(SideTarget::File(self.diff_pane.file)),
        ));
        self.focus = Focus::Files;
    }

    pub(super) fn filter_key(&mut self, key: KeyEvent) -> Effect {
        let view = FileTreeView {
            files: &self.session.files,
            comments: &self.session.comments,
            reviewed_files: &self.session.reviewed_files,
            current_file: self.diff_pane.file,
            active_comment: self.active_comment_index(),
            focused: true,
            divided: true,
        };
        match self.file_tree.search(key, &view) {
            SearchAction::Cancel => self.focus = self.search_return_focus,
            SearchAction::Accept(Some(file)) => {
                self.select_side_target(SideTarget::File(file), true)
            }
            SearchAction::Accept(None) | SearchAction::None => {}
        }
        Effect::None
    }
}
