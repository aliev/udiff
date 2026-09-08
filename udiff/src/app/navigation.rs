//! File selection and sidebar focus.

use super::{App, Focus, file_tree::Target as SideTarget, view_helpers::anchor_position};
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
}
