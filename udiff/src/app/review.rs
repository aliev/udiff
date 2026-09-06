//! User-facing review workflows: comments, suggestions, and reviewed files.

use super::{
    App, Focus,
    command::Effect,
    comment_editor::{Action as EditorAction, Mode as EditorMode},
    file_tree::Target as SideTarget,
    view_helpers::{anchor_position, line_in_comment},
};
use crate::{
    comment::{Comment, format_for_clipboard},
    model::{FileDiff, LineKind},
};
use crossterm::event::KeyEvent;

impl App {
    pub(super) fn current(&self) -> &FileDiff {
        self.diff_pane.current(&self.session.files)
    }

    pub(super) fn selected_bounds(&self) -> (usize, usize) {
        self.diff_pane.selected_bounds()
    }

    pub(super) fn comment_at(&self, position: usize) -> Option<&Comment> {
        let path = &self.current().path;
        self.session
            .comments
            .iter()
            .find(|comment| {
                comment.path == *path && anchor_position(self.current(), comment) == Some(position)
            })
            .or_else(|| {
                self.session.comments.iter().find(|comment| {
                    comment.path == *path
                        && line_in_comment(&self.current().lines[position], comment)
                })
            })
    }

    fn anchored_comment_at(&self, position: usize) -> Option<&Comment> {
        let path = &self.current().path;
        self.session.comments.iter().find(|comment| {
            comment.path == *path && anchor_position(self.current(), comment) == Some(position)
        })
    }

    fn anchored_suggestion_at(&self, position: usize) -> Option<&Comment> {
        self.anchored_comment_at(position)
            .filter(|comment| comment.body.is_suggestion())
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
                self.file_tree.select(Some(SideTarget::File(next)));
            }
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

    pub(super) fn editor_key(&mut self, key: KeyEvent) -> Effect {
        match self.comment_editor.event(key) {
            EditorAction::Cancel => {
                self.focus = Focus::Diff;
                self.comment_editor.close();
            }
            EditorAction::Save => self.save_editor(),
            EditorAction::None => {}
        }
        Effect::None
    }

    pub(super) fn open_editor(&mut self) {
        if self.current().lines[self.diff_pane.cursor].kind == LineKind::Meta {
            return;
        }
        let existing = self.anchored_comment_at(self.diff_pane.cursor).cloned();
        let text = existing
            .as_ref()
            .map_or(String::new(), |comment| comment.first_text().to_owned());
        let mode = existing.as_ref().map_or(EditorMode::Comment, |comment| {
            if comment.body.is_suggestion() {
                EditorMode::Suggestion
            } else {
                EditorMode::Comment
            }
        });
        let range_bottom = self.selected_bounds().1;
        let anchor = existing
            .as_ref()
            .and_then(|comment| anchor_position(self.current(), comment))
            .unwrap_or(range_bottom);
        self.comment_editor
            .open(text, anchor, existing.as_ref().map(Comment::key), mode);
        self.focus = Focus::Editor;
    }

    pub(super) fn open_suggestion_editor(&mut self) {
        // On a removal with nothing selected, suggest against what replaced it
        // rather than refusing. An explicit range is the reviewer's own choice
        // and is left alone.
        if self.diff_pane.range_anchor.is_none() {
            self.diff_pane.step_to_replacement();
        }
        let range = self.selected_bounds();
        let existing = self
            .anchored_suggestion_at(self.diff_pane.cursor)
            .filter(|_| self.diff_pane.range_anchor.is_none())
            .cloned();
        let replacement = if let Some(comment) = &existing {
            comment.first_text().to_owned()
        } else {
            match self
                .session
                .suggestion_replacement(self.diff_pane.file, range)
            {
                Ok(replacement) => replacement,
                Err(error) => {
                    self.notice(error.message());
                    return;
                }
            }
        };
        let anchor = existing
            .as_ref()
            .and_then(|comment| anchor_position(self.current(), comment))
            .unwrap_or(range.1);
        self.comment_editor.open(
            replacement,
            anchor,
            existing.as_ref().map(Comment::key),
            EditorMode::Suggestion,
        );
        self.focus = Focus::Editor;
    }

    pub(super) fn save_editor(&mut self) {
        let range = self.selected_bounds();
        let anchor = self.comment_editor.anchor.unwrap_or(range.1);
        let editing = self.comment_editor.editing_key.is_some();
        let mode = self.comment_editor.mode;
        let text = self.comment_editor.text.clone();
        let editing_key = self.comment_editor.editing_key.clone();
        let has_text = !text.trim().is_empty();
        match mode {
            EditorMode::Comment => {
                if !self.session.save_comment(
                    self.diff_pane.file,
                    range,
                    anchor,
                    editing_key,
                    &text,
                ) {
                    return;
                }
            }
            EditorMode::Suggestion => {
                if let Err(error) = self.session.save_suggestion(
                    self.diff_pane.file,
                    range,
                    anchor,
                    editing_key,
                    &text,
                ) {
                    self.notice(error.message());
                    return;
                }
            }
        }
        self.diff_pane.range_anchor = None;
        self.focus = Focus::Diff;
        self.comment_editor.close();
        self.notice(if mode == EditorMode::Suggestion && text.is_empty() {
            "deletion suggestion saved · Space when file is ready"
        } else if mode == EditorMode::Suggestion && editing {
            "suggestion updated"
        } else if mode == EditorMode::Suggestion {
            "suggestion saved · Space when file is ready"
        } else if !has_text {
            "empty comment discarded"
        } else if editing {
            "comment updated"
        } else {
            "comment saved · Space when file is ready"
        });
    }

    pub(super) fn delete_comment_at_cursor(&mut self) {
        if let Some(key) = self.comment_at(self.diff_pane.cursor).map(Comment::key)
            && self.session.delete_comment(&key)
        {
            if self.focus == Focus::Files {
                self.file_tree
                    .select(Some(SideTarget::File(self.diff_pane.file)));
            }
            self.notice("comment deleted · u undo");
        }
    }

    pub(super) fn undo_delete_comment(&mut self) {
        if let Some(restored) = self.session.restore_comment() {
            if self.focus == Focus::Files {
                self.file_tree.select(Some(SideTarget::Comment {
                    file: self.diff_pane.file,
                    comment: restored,
                }));
            }
            self.notice("deleted comment restored");
        }
    }

    pub(super) fn jump_comment(&mut self, next: bool) {
        let mut positions = self
            .session
            .comments
            .iter()
            .filter(|comment| comment.path == self.current().path)
            .filter_map(|comment| anchor_position(self.current(), comment))
            .collect::<Vec<_>>();
        positions.sort_unstable();
        if positions.is_empty() {
            return;
        }
        self.diff_pane.cursor = if next {
            positions
                .iter()
                .copied()
                .find(|position| *position > self.diff_pane.cursor)
                .unwrap_or(positions[0])
        } else {
            positions
                .iter()
                .rev()
                .copied()
                .find(|position| *position < self.diff_pane.cursor)
                .unwrap_or(*positions.last().expect("positions is not empty"))
        };
    }
}
