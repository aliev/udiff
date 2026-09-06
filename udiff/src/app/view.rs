//! Top-level screen layout. Individual widgets render themselves.

use super::{App, Focus, file_tree::View as FileTreeView, statusline::View as StatuslineView};
use crate::theme::theme;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::Style,
    widgets::Block,
};

/// Share of the body the explorer may take before it stops growing.
const SIDEBAR_SHARE_PERCENT: u16 = 28;
const SIDEBAR_MIN_WIDTH: u16 = 22;
const SIDEBAR_MAX_WIDTH: u16 = 40;
/// Below this width the two panes cannot both stay readable, so only the
/// focused one is shown and `-` / `Tab` swaps between them.
const NARROW_BODY_WIDTH: u16 = 64;

/// Explorer width for a body of `total` columns. Returns `0` when the explorer
/// is gone and `total` when it takes the screen on its own. An explicit `hidden`
/// outranks the narrow-terminal rule: asked to be gone means gone.
pub(super) fn sidebar_width(total: u16, files_focused: bool, hidden: bool) -> u16 {
    if hidden {
        return 0;
    }
    if total < NARROW_BODY_WIDTH {
        return if files_focused { total } else { 0 };
    }
    (total * SIDEBAR_SHARE_PERCENT / 100)
        .clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH)
        .min(total)
}

impl App {
    pub fn draw(&mut self, frame: &mut Frame) {
        let root = frame.area();
        frame.render_widget(
            Block::default().style(Style::default().fg(theme().text).bg(theme().bg)),
            root,
        );
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(8), Constraint::Length(1)])
            .split(root);
        let focused = matches!(self.focus, Focus::Files | Focus::Filter);
        let sidebar = sidebar_width(rows[0].width, focused, self.sidebar_hidden);
        let body = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(sidebar), Constraint::Min(0)])
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
                focused,
                divided: body[1].width > 0,
                width: body[0].width,
            },
        );
        self.diff_pane.draw(
            frame,
            body[1],
            &self.session,
            &self.comment_editor,
            self.focus,
            self.sidebar_hidden,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidebar_grows_with_the_body_between_its_bounds() {
        assert_eq!(sidebar_width(80, false, false), SIDEBAR_MIN_WIDTH);
        assert_eq!(sidebar_width(120, false, false), 33);
        assert_eq!(sidebar_width(240, false, false), SIDEBAR_MAX_WIDTH);
    }

    #[test]
    fn narrow_bodies_show_only_the_focused_pane() {
        assert_eq!(sidebar_width(50, true, false), 50);
        assert_eq!(sidebar_width(50, false, false), 0);
    }

    #[test]
    fn hiding_the_explorer_outranks_every_other_rule() {
        // Wide, narrow, focused, unfocused: asked to be gone means gone.
        for (total, focused) in [(240, false), (120, true), (50, true), (50, false)] {
            assert_eq!(sidebar_width(total, focused, true), 0, "{total}/{focused}");
        }
    }
}
