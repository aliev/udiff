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
    pub text: String,
}

impl Comment {
    pub fn key(&self) -> String {
        self.id.clone()
    }

    pub fn first_text(&self) -> &str {
        &self.text
    }
}

pub fn format_for_clipboard(comments: &[Comment]) -> String {
    let mut output = String::new();
    for (index, comment) in comments.iter().enumerate() {
        output.push_str(&format!(
            "{}{}. {} ({})\nSelected diff:\n{}\nComment: {}",
            if output.is_empty() { "" } else { "\n\n" },
            index + 1,
            comment.path,
            location(comment),
            comment.excerpt.trim_end(),
            comment.text.trim()
        ));
    }
    output
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
            text: "Please simplify".into(),
        };

        assert_eq!(
            format_for_clipboard(&[comment]),
            "1. src/main.rs (old lines 3; new lines 3-4)\nSelected diff:\n-old\n+new\nComment: Please simplify"
        );
    }
}
