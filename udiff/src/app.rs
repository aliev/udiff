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
