#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommentBody {
    Text(String),
    Suggestion { replacement: String },
}

impl CommentBody {
    pub fn text(&self) -> &str {
        match self {
            Self::Text(text) => text,
            Self::Suggestion { replacement } => replacement,
        }
    }

    pub fn is_suggestion(&self) -> bool {
        matches!(self, Self::Suggestion { .. })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Comment {
    pub id: String,
    pub path: String,
    pub excerpt: String,
    pub old_start: Option<u32>,
    pub old_end: Option<u32>,
    pub new_start: Option<u32>,
    pub new_end: Option<u32>,
    pub anchor_old: Option<u32>,
    pub anchor_new: Option<u32>,
    pub body: CommentBody,
}

impl Comment {
    pub fn key(&self) -> String {
        self.id.clone()
    }

    pub fn first_text(&self) -> &str {
        self.body.text()
    }

    pub fn short_location(&self) -> String {
        let range = |start: Option<u32>, end: Option<u32>| match (start, end) {
            (Some(start), Some(end)) if start != end => Some(format!("{start}–{end}")),
            (Some(start), _) => Some(start.to_string()),
            _ => None,
        };
        if let Some(lines) = range(self.new_start, self.new_end) {
            format!("L{lines}")
        } else if let Some(lines) = range(self.old_start, self.old_end) {
            format!("old L{lines}")
        } else {
            "hunk".into()
        }
    }
}

pub fn format_for_clipboard(comments: &[Comment]) -> String {
    let mut output = String::new();
    for (index, comment) in comments.iter().enumerate() {
        output.push_str(&format!(
            "{}{}. {} ({})\nSelected diff:\n{}\n{}",
            if output.is_empty() { "" } else { "\n\n" },
            index + 1,
            comment.path,
            location(comment),
            comment.excerpt.trim_end(),
            format_body(&comment.body),
        ));
    }
    output
}

fn format_body(body: &CommentBody) -> String {
    match body {
        CommentBody::Text(text) => format!("Comment: {}", text.trim()),
        CommentBody::Suggestion { replacement } => {
            let fence = suggestion_fence(replacement);
            format!("Suggested replacement:\n{fence}suggestion\n{replacement}\n{fence}")
        }
    }
}

fn suggestion_fence(replacement: &str) -> String {
    let longest_run = replacement
        .split(|character| character != '`')
        .map(str::len)
        .max()
        .unwrap_or(0);
    "`".repeat(3.max(longest_run + 1))
}

fn location(comment: &Comment) -> String {
    let range = |start: Option<u32>, end: Option<u32>| match (start, end) {
        (Some(start), Some(end)) if start != end => Some(format!("{start}-{end}")),
        (Some(start), _) => Some(start.to_string()),
        _ => None,
    };
    match (
        range(comment.old_start, comment.old_end),
        range(comment.new_start, comment.new_end),
    ) {
        (Some(old), Some(new)) => format!("old lines {old}; new lines {new}"),
        (Some(old), None) => format!("old lines {old}"),
        (None, Some(new)) => format!("new lines {new}"),
        (None, None) => "hunk header".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_format_contains_only_location_excerpt_and_text() {
        let comment = Comment {
            id: "c-001".into(),
            path: "src/main.rs".into(),
            excerpt: "-old\n+new".into(),
            old_start: Some(3),
            old_end: Some(3),
            new_start: Some(3),
            new_end: Some(4),
            anchor_old: None,
            anchor_new: Some(4),
            body: CommentBody::Text("Please simplify".into()),
        };

        assert_eq!(
            format_for_clipboard(std::slice::from_ref(&comment)),
            "1. src/main.rs (old lines 3; new lines 3-4)\nSelected diff:\n-old\n+new\nComment: Please simplify"
        );
        assert_eq!(comment.short_location(), "L3–4");
    }

    #[test]
    fn clipboard_format_preserves_suggestion_whitespace() {
        let suggestion = Comment {
            id: "s-001".into(),
            path: "src/main.rs".into(),
            excerpt: "+    old();".into(),
            old_start: None,
            old_end: None,
            new_start: Some(3),
            new_end: Some(3),
            anchor_old: None,
            anchor_new: Some(3),
            body: CommentBody::Suggestion {
                replacement: "    new();\n".into(),
            },
        };

        assert_eq!(
            format_for_clipboard(&[suggestion]),
            "1. src/main.rs (new lines 3)\nSelected diff:\n+    old();\nSuggested replacement:\n```suggestion\n    new();\n\n```"
        );
    }

    #[test]
    fn suggestion_fence_is_longer_than_backticks_in_the_replacement() {
        assert_eq!(suggestion_fence("let fence = \"```\";"), "````");
    }
}
