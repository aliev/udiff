use ratatui::style::Color;

mod command;
mod comment_editor;
mod controller;
mod diff_pane;
mod diff_view;
mod editor;
mod file_tree;
mod help;
mod navigation;
mod render;
mod review;
mod revisions;
mod search;
mod session;
mod statusline;
mod view;
mod view_helpers;
pub use command::{Command, EditorTarget, Effect};
use comment_editor::CommentEditor;
use diff_pane::DiffPane;
use file_tree::FileTree;
use help::Help;
use revisions::RevisionState;
use session::Session;
use statusline::Statusline;

pub(crate) const BG: Color = Color::Rgb(15, 18, 25);
pub(crate) const SURFACE: Color = Color::Rgb(21, 25, 35);
pub(crate) const BORDER: Color = Color::Rgb(45, 51, 66);
pub(crate) const TEXT: Color = Color::Rgb(220, 224, 232);
pub(crate) const MUTED: Color = Color::Rgb(122, 132, 153);
const BLUE: Color = Color::Rgb(122, 162, 247);
const GREEN: Color = Color::Rgb(158, 206, 106);
const GREEN_BG: Color = Color::Rgb(24, 45, 35);
const RED: Color = Color::Rgb(247, 118, 142);
const RED_BG: Color = Color::Rgb(54, 31, 40);
const HUNK_BG: Color = Color::Rgb(28, 38, 58);
const COMMENT: Color = Color::Rgb(224, 175, 104);
const COMMENT_BG: Color = Color::Rgb(47, 39, 28);
const SELECT_BG: Color = Color::Rgb(38, 49, 70);
const TAB_WIDTH: usize = 4;
const PREVIOUS_BATCH_KEY: char = '{';
const NEXT_BATCH_KEY: char = '}';

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Focus {
    Files,
    Diff,
    Filter,
    Editor,
}
pub struct App {
    // Current review.
    session: Session,
    diff_pane: DiffPane,
    file_tree: FileTree,
    comment_editor: CommentEditor,

    // Shared UI state.
    focus: Focus,
    search_return_focus: Focus,
    help: Help,
    statusline: Statusline,

    // Watch-mode history. The active revision uses the fields above.
    revision_number: u64,
    revision_states: Vec<Option<RevisionState>>,
    active_revision: usize,
    watching: bool,
}

#[cfg(test)]
mod tests;
