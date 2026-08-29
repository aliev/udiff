use ratatui::style::Color;

mod command;
mod comment_editor;
mod controller;
mod diff_pane;
mod diff_view;
mod editor;
mod file_tree;
mod help;
mod render;
mod search;
mod session;
mod statusline;
mod view_helpers;
pub use command::{Command, EditorTarget, Effect, RevisionOrigin};
use comment_editor::CommentEditor;
use diff_pane::DiffPane;
use file_tree::FileTree;
use help::Help;
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
enum Outcome {
    Continue,
    Finish,
    ResetWatch,
    Yank(String),
    LoadFileView(usize),
    OpenEditor(EditorTarget),
}

pub struct App {
    session: Session,
    diff_pane: DiffPane,
    focus: Focus,
    search_return_focus: Focus,
    comment_editor: CommentEditor,
    file_tree: FileTree,
    help: Help,
    statusline: Statusline,
    batch_number: u64,
    revision_origin: RevisionOrigin,
    batch_states: Vec<Option<BatchState>>,
    active_batch: usize,
    watching: bool,
}

struct BatchState {
    number: u64,
    origin: RevisionOrigin,
    session: Session,
    diff_pane: DiffPane,
    file_tree: FileTree,
}

impl BatchState {
    fn new(number: u64, files: Vec<crate::model::FileDiff>, origin: RevisionOrigin) -> Self {
        Self {
            number,
            origin,
            diff_pane: DiffPane::new(&files),
            session: Session::new(files, Vec::new()),
            file_tree: FileTree::default(),
        }
    }
}

#[cfg(test)]
mod tests;
