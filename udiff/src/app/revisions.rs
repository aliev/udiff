//! Independent review state for revisions received in watch mode.

use super::{App, DiffPane, FileTree, Focus, Session};
use crate::{comment::Comment, model::FileDiff};

/// Everything that belongs to one watched revision.
///
/// The active revision lives directly on `App`; inactive revisions are stored
/// as `RevisionState`s and swapped back in when the user presses `{` or `}`.
pub(super) struct RevisionState {
    pub number: u64,
    pub session: Session,
    pub diff_pane: DiffPane,
    pub file_tree: FileTree,
}

impl RevisionState {
    pub fn new(number: u64, files: Vec<FileDiff>) -> Self {
        Self {
            number,
            diff_pane: DiffPane::new(&files),
            session: Session::new(files, Vec::new()),
            file_tree: FileTree::default(),
        }
    }
}

impl App {
    #[cfg_attr(not(feature = "watch"), allow(dead_code))]
    pub fn new_watching(number: u64, files: Vec<FileDiff>) -> Self {
        let mut app = Self::new(files, Vec::new());
        app.revision_number = number;
        app.watching = true;
        app
    }

    #[cfg_attr(not(feature = "watch"), allow(dead_code))]
    pub fn new_watching_context(files: Vec<FileDiff>) -> Self {
        let mut app = Self::new(files, Vec::new());
        app.revision_number = 0;
        app.watching = true;
        app
    }

    pub(super) fn push_revision(&mut self, number: u64, files: Vec<FileDiff>) {
        let was_latest = self.active_revision + 1 == self.revision_states.len();
        self.revision_states
            .push(Some(RevisionState::new(number, files)));
        if was_latest {
            self.switch_revision(self.revision_states.len() - 1);
        } else {
            let newer = self.revision_states.len() - self.active_revision - 1;
            self.notice(format!("{newer} newer revision(s)"));
        }
    }

    pub(super) fn switch_revision(&mut self, target: usize) {
        if target == self.active_revision || target >= self.revision_states.len() {
            return;
        }
        let Some(target_state) = self.revision_states[target].take() else {
            return;
        };
        let RevisionState {
            number,
            session,
            diff_pane,
            file_tree,
        } = target_state;
        let previous = RevisionState {
            number: std::mem::replace(&mut self.revision_number, number),
            session: std::mem::replace(&mut self.session, session),
            diff_pane: std::mem::replace(&mut self.diff_pane, diff_pane),
            file_tree: std::mem::replace(&mut self.file_tree, file_tree),
        };
        self.diff_pane.adopt_view(&previous.diff_pane);
        self.revision_states[self.active_revision] = Some(previous);
        self.active_revision = target;
        self.focus = Focus::Diff;
        self.comment_editor.close();
    }

    pub(super) fn comments_for_clipboard(&self) -> Vec<Comment> {
        if !self.watching {
            return self.session.comments_for_unreviewed_files();
        }

        self.revision_states
            .iter()
            .enumerate()
            .flat_map(|(index, state)| {
                if index == self.active_revision {
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
}
