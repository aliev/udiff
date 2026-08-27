use crate::{
    comment::Comment,
    model::{FileDiff, LineKind, hunk_ranges},
};
use std::collections::HashSet;

pub struct Session {
    pub files: Vec<FileDiff>,
    pub comments: Vec<Comment>,
    pub viewed_files: HashSet<usize>,
    deleted_comments: Vec<(usize, Comment)>,
}

impl Session {
    pub fn new(files: Vec<FileDiff>, comments: Vec<Comment>) -> Self {
        Self {
            files,
            comments,
            viewed_files: HashSet::new(),
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

    pub fn unviewed_comments(&self) -> Vec<Comment> {
        self.comments
            .iter()
            .filter(|comment| {
                self.files
                    .iter()
                    .position(|file| file.path == comment.path)
                    .is_none_or(|file| !self.viewed_files.contains(&file))
            })
            .cloned()
            .collect()
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
                comment.text = text.into();
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
            text: text.into(),
        });
        true
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
            text: "note".into(),
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
    fn comments_from_viewed_files_are_excluded_from_review() {
        let files = parse_unified_diff(
            "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-old\n+new\n\
             diff --git a/b.rs b/b.rs\n--- a/b.rs\n+++ b/b.rs\n@@ -1 +1 @@\n-old\n+new\n",
        );
        let mut first = comment("t1");
        first.path = "a.rs".into();
        let mut second = comment("t2");
        second.path = "b.rs".into();
        let mut session = Session::new(files, vec![first, second]);
        session.viewed_files.insert(0);

        assert_eq!(
            session
                .unviewed_comments()
                .iter()
                .map(|comment| comment.path.as_str())
                .collect::<Vec<_>>(),
            ["b.rs"]
        );
    }
}
