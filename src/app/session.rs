use crate::{
    comment::{Comment, CommentBody},
    model::{FileDiff, LineKind, hunk_ranges},
};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SuggestionError {
    OldSideSelection,
    NonContiguousRange,
}

impl SuggestionError {
    pub fn message(self) -> &'static str {
        match self {
            Self::OldSideSelection => "suggestions require new or context lines",
            Self::NonContiguousRange => "suggestions require contiguous new lines",
        }
    }
}

pub struct Session {
    pub files: Vec<FileDiff>,
    pub comments: Vec<Comment>,
    pub reviewed_files: HashSet<usize>,
    deleted_comments: Vec<(usize, Comment)>,
}

impl Session {
    pub fn new(files: Vec<FileDiff>, comments: Vec<Comment>) -> Self {
        Self {
            files,
            comments,
            reviewed_files: HashSet::new(),
            deleted_comments: Vec::new(),
        }
    }

    pub fn delete_comment(&mut self, key: &str) -> bool {
        let Some(index) = self
            .comments
            .iter()
            .position(|comment| comment.key() == key)
        else {
            return false;
        };
        let comment = self.comments.remove(index);
        self.deleted_comments.push((index, comment));
        true
    }

    pub fn restore_comment(&mut self) -> Option<usize> {
        let (index, comment) = self.deleted_comments.pop()?;
        let restored = index.min(self.comments.len());
        self.comments.insert(restored, comment);
        Some(restored)
    }

    pub fn push_comment(&mut self, comment: Comment) {
        self.deleted_comments.clear();
        self.comments.push(comment);
    }

    pub fn comments_for_unreviewed_files(&self) -> Vec<Comment> {
        self.comments
            .iter()
            .filter(|comment| {
                self.files
                    .iter()
                    .position(|file| file.path == comment.path)
                    .is_none_or(|file| !self.reviewed_files.contains(&file))
            })
            .cloned()
            .collect()
    }

    pub fn next_unreviewed_file(&self, current: usize) -> Option<usize> {
        (1..=self.files.len())
            .map(|offset| (current + offset) % self.files.len())
            .find(|file| !self.reviewed_files.contains(file))
    }

    pub fn save_comment(
        &mut self,
        file: usize,
        range: (usize, usize),
        anchor: usize,
        editing_key: Option<String>,
        text: &str,
    ) -> bool {
        let text = text.trim();
        if text.is_empty() {
            return true;
        }
        if let Some(key) = editing_key {
            if let Some(comment) = self.comments.iter_mut().find(|comment| comment.id == key) {
                comment.body = CommentBody::Text(text.into());
            }
            return true;
        }
        let file_diff = &self.files[file];
        let lines = &file_diff.lines[range.0..=range.1];
        if lines.iter().any(|line| line.kind == LineKind::Meta) {
            return false;
        }
        let numbers = |old: bool| {
            lines
                .iter()
                .filter_map(|line| if old { line.old } else { line.new })
                .collect::<Vec<_>>()
        };
        let mut old = numbers(true);
        let mut new = numbers(false);
        if old.is_empty()
            && new.is_empty()
            && lines.len() == 1
            && let Some((old_start, old_end, new_start, new_end)) = hunk_ranges(&lines[0].text)
        {
            old.extend([old_start, old_end]);
            new.extend([new_start, new_end]);
        }
        let excerpt = lines
            .iter()
            .map(|line| {
                let marker = match line.kind {
                    LineKind::Hunk => "",
                    LineKind::Add => "+",
                    LineKind::Remove => "-",
                    _ => " ",
                };
                format!("{marker}{}", line.text)
            })
            .collect::<Vec<_>>()
            .join("\n");
        let anchor_line = &file_diff.lines[anchor];
        let id = next_id("t", self.comments.iter().map(|comment| comment.id.as_str()));
        self.push_comment(Comment {
            id,
            path: file_diff.path.clone(),
            excerpt,
            old_start: old.first().copied(),
            old_end: old.last().copied(),
            new_start: new.first().copied(),
            new_end: new.last().copied(),
            anchor_old: anchor_line.old,
            anchor_new: anchor_line.new,
            body: CommentBody::Text(text.into()),
        });
        true
    }

    pub fn suggestion_replacement(
        &self,
        file: usize,
        range: (usize, usize),
    ) -> Result<String, SuggestionError> {
        let lines = &self.files[file].lines[range.0..=range.1];
        if lines
            .iter()
            .any(|line| !matches!(line.kind, LineKind::Context | LineKind::Add))
        {
            return Err(SuggestionError::OldSideSelection);
        }
        let numbers = lines
            .iter()
            .map(|line| line.new.ok_or(SuggestionError::OldSideSelection))
            .collect::<Result<Vec<_>, _>>()?;
        if numbers.windows(2).any(|pair| pair[1] != pair[0] + 1) {
            return Err(SuggestionError::NonContiguousRange);
        }
        Ok(lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n"))
    }

    pub fn save_suggestion(
        &mut self,
        file: usize,
        range: (usize, usize),
        anchor: usize,
        editing_key: Option<String>,
        replacement: &str,
    ) -> Result<(), SuggestionError> {
        if let Some(key) = editing_key {
            if let Some(comment) = self.comments.iter_mut().find(|comment| comment.id == key) {
                comment.body = CommentBody::Suggestion {
                    replacement: replacement.into(),
                };
            }
            return Ok(());
        }

        self.suggestion_replacement(file, range)?;
        let file_diff = &self.files[file];
        let lines = &file_diff.lines[range.0..=range.1];
        let excerpt = lines
            .iter()
            .map(|line| format!("{}{}", line.marker(), line.text))
            .collect::<Vec<_>>()
            .join("\n");
        let anchor_new = file_diff.lines[anchor].new;
        let id = next_id("s", self.comments.iter().map(|comment| comment.id.as_str()));
        self.push_comment(Comment {
            id,
            path: file_diff.path.clone(),
            excerpt,
            old_start: None,
            old_end: None,
            new_start: lines.first().and_then(|line| line.new),
            new_end: lines.last().and_then(|line| line.new),
            anchor_old: None,
            anchor_new,
            body: CommentBody::Suggestion {
                replacement: replacement.into(),
            },
        });
        Ok(())
    }
}

fn next_id<'a>(prefix: &str, existing: impl Iterator<Item = &'a str>) -> String {
    let existing = existing.collect::<HashSet<_>>();
    (1..)
        .map(|number| format!("{prefix}-{number:03}"))
        .find(|candidate| !existing.contains(candidate.as_str()))
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::parse_unified_diff;

    fn comment(id: &str) -> Comment {
        Comment {
            id: id.into(),
            path: "a.rs".into(),
            excerpt: "x".into(),
            old_start: Some(1),
            old_end: Some(1),
            new_start: Some(1),
            new_end: Some(1),
            anchor_old: Some(1),
            anchor_new: Some(1),
            body: CommentBody::Text("comment".into()),
        }
    }

    #[test]
    fn comment_history_is_owned_by_the_session() {
        let mut session = Session::new(Vec::new(), vec![comment("t1"), comment("t2")]);
        assert!(session.delete_comment("t1"));
        assert_eq!(
            session
                .comments
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            vec!["t2"]
        );
        assert_eq!(session.restore_comment(), Some(0));
        assert_eq!(
            session
                .comments
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            vec!["t1", "t2"]
        );
    }

    #[test]
    fn comments_from_reviewed_files_are_excluded_from_export() {
        let files = parse_unified_diff(
            "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-old\n+new\n\
             diff --git a/b.rs b/b.rs\n--- a/b.rs\n+++ b/b.rs\n@@ -1 +1 @@\n-old\n+new\n",
        );
        let mut first = comment("t1");
        first.path = "a.rs".into();
        let mut second = comment("t2");
        second.path = "b.rs".into();
        let mut session = Session::new(files, vec![first, second]);
        session.reviewed_files.insert(0);

        assert_eq!(
            session
                .comments_for_unreviewed_files()
                .iter()
                .map(|comment| comment.path.as_str())
                .collect::<Vec<_>>(),
            ["b.rs"]
        );
    }

    #[test]
    fn next_unreviewed_file_wraps_and_skips_reviewed_files() {
        let files = parse_unified_diff(
            "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-a\n+b\n\
             diff --git a/b.rs b/b.rs\n--- a/b.rs\n+++ b/b.rs\n@@ -1 +1 @@\n-a\n+b\n\
             diff --git a/c.rs b/c.rs\n--- a/c.rs\n+++ b/c.rs\n@@ -1 +1 @@\n-a\n+b\n",
        );
        let mut session = Session::new(files, Vec::new());
        session.reviewed_files.extend([0, 2]);

        assert_eq!(session.next_unreviewed_file(2), Some(1));
        session.reviewed_files.insert(1);
        assert_eq!(session.next_unreviewed_file(2), None);
    }

    #[test]
    fn suggestion_source_accepts_only_contiguous_new_side_lines() {
        let files = parse_unified_diff(
            "--- a.rs\n+++ a.rs\n@@ -1,3 +1,3 @@\n-old\n+    new\n context\n tail\n",
        );
        let session = Session::new(files, Vec::new());

        assert_eq!(
            session.suggestion_replacement(0, (2, 3)),
            Ok("    new\ncontext".into())
        );
        assert_eq!(
            session.suggestion_replacement(0, (1, 2)),
            Err(SuggestionError::OldSideSelection)
        );
    }

    #[test]
    fn empty_suggestion_is_saved_as_a_deletion_without_trimming() {
        let files = parse_unified_diff("--- a.rs\n+++ a.rs\n@@ -0,0 +1 @@\n+    old\n");
        let mut session = Session::new(files, Vec::new());

        session.save_suggestion(0, (1, 1), 1, None, "").unwrap();
        assert_eq!(
            session.comments[0].body,
            CommentBody::Suggestion {
                replacement: String::new()
            }
        );

        let key = session.comments[0].key();
        session
            .save_suggestion(0, (1, 1), 1, Some(key), "    new\n")
            .unwrap();
        assert_eq!(
            session.comments[0].body,
            CommentBody::Suggestion {
                replacement: "    new\n".into()
            }
        );
    }
}
