//! Top-level screen layout. Individual widgets render themselves.

use super::{App, BG, Focus, file_tree::View as FileTreeView, statusline::View as StatuslineView};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::Style,
    widgets::Block,
};

impl App {
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

        self.file_tree.draw(
            frame,
            body[0],
            &FileTreeView {
                files: &self.session.files,
                comments: &self.session.comments,
                reviewed_files: &self.session.reviewed_files,
                current_file: self.diff_pane.file,
                active_comment: self.active_comment_index(),
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
                editor: &self.comment_editor,
                revision: self.watching.then_some((
                    self.active_revision + 1,
                    self.revision_states.len(),
                    self.revision_number,
                    self.revision_number == 0,
                )),
            },
        );
        self.help.draw(frame, root);
    }
}
