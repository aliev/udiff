use regex::Regex;

mod highlight;
use highlight::apply as highlight;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LineKind {
    Context,
    Add,
    Remove,
    Hunk,
    Meta,
}

#[derive(Clone, Debug)]
pub struct SyntaxSpan {
    pub text: String,
    pub rgb: (u8, u8, u8),
    pub bold: bool,
    pub italic: bool,
}

#[derive(Clone, Debug)]
pub struct DiffLine {
    pub text: String,
    pub kind: LineKind,
    pub old: Option<u32>,
    pub new: Option<u32>,
    pub syntax: Vec<SyntaxSpan>,
}

impl DiffLine {
    pub fn marker(&self) -> char {
        match self.kind {
            LineKind::Add => '+',
            LineKind::Remove => '-',
            _ => ' ',
        }
    }
    pub fn review_line(&self) -> Option<u32> {
        if self.kind == LineKind::Remove {
            self.old
        } else {
            self.new
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileStatus {
    Added,
    Deleted,
    Renamed,
    Modified,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileViewChange {
    Added,
    Modified,
    Deleted,
    Removed,
}

#[derive(Clone, Debug)]
pub struct FileDiff {
    pub path: String,
    pub old_path: Option<String>,
    pub status: FileStatus,
    pub lines: Vec<DiffLine>,
}

pub fn hunk_ranges(header: &str) -> Option<(u32, u32, u32, u32)> {
    let captures = Regex::new(r"^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@")
        .ok()?
        .captures(header)?;
    let old_start = captures[1].parse().ok()?;
    let old_count: u32 = captures
        .get(2)
        .and_then(|m| m.as_str().parse().ok())
        .unwrap_or(1);
    let new_start = captures[3].parse().ok()?;
    let new_count: u32 = captures
        .get(4)
        .and_then(|m| m.as_str().parse().ok())
        .unwrap_or(1);
    Some((
        old_start,
        old_start.saturating_add(old_count.saturating_sub(1)),
        new_start,
        new_start.saturating_add(new_count.saturating_sub(1)),
    ))
}

impl FileDiff {
    pub fn additions(&self) -> usize {
        self.lines
            .iter()
            .filter(|l| l.kind == LineKind::Add)
            .count()
    }
    pub fn deletions(&self) -> usize {
        self.lines
            .iter()
            .filter(|l| l.kind == LineKind::Remove)
            .count()
    }
}

pub fn file_view_changes(file: &FileDiff) -> Vec<(u32, FileViewChange)> {
    let mut changes = Vec::new();
    let last_new_line = file
        .lines
        .iter()
        .filter_map(|line| line.new)
        .max()
        .unwrap_or(1);
    let mut position = 0;
    let mut hunk_new_start = 1;
    while position < file.lines.len() {
        if file.lines[position].kind == LineKind::Hunk {
            if let Some((_, _, start, _)) = hunk_ranges(&file.lines[position].text) {
                hunk_new_start = start;
            }
            position += 1;
            continue;
        }
        if !matches!(file.lines[position].kind, LineKind::Add | LineKind::Remove) {
            position += 1;
            continue;
        }
        let start = position;
        while position < file.lines.len()
            && matches!(file.lines[position].kind, LineKind::Add | LineKind::Remove)
        {
            position += 1;
        }
        let block = &file.lines[start..position];
        let has_removals = block.iter().any(|line| line.kind == LineKind::Remove);
        let additions = block
            .iter()
            .filter(|line| line.kind == LineKind::Add)
            .filter_map(|line| line.new)
            .collect::<Vec<_>>();
        if additions.is_empty() && has_removals {
            let next = file.lines[position..].iter().find_map(|line| line.new);
            let previous = file.lines[..start]
                .iter()
                .rev()
                .find_map(|line| line.new)
                .map(|line| line.saturating_add(1));
            let anchor = next
                .or(previous)
                .unwrap_or(hunk_new_start)
                .max(1)
                .min(last_new_line);
            changes.push((anchor, FileViewChange::Deleted));
        } else {
            changes.extend(additions.into_iter().map(|line| {
                (
                    line,
                    if has_removals {
                        FileViewChange::Modified
                    } else {
                        FileViewChange::Added
                    },
                )
            }));
        }
    }
    changes
}

pub fn parse_unified_diff(raw: &str) -> Vec<FileDiff> {
    let hunk = Regex::new(r"^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@").unwrap();
    let mut files = Vec::new();
    let mut current: Option<FileDiff> = None;
    let mut old_path: Option<String> = None;
    let mut status = FileStatus::Modified;
    let (mut old_no, mut new_no) = (0, 0);
    for raw_line in raw.lines() {
        if raw_line.starts_with("diff --git ") {
            if let Some(file) = current.take() {
                files.push(file);
            }
            old_path = None;
            status = FileStatus::Modified;
            continue;
        }
        if raw_line.starts_with("new file mode ") {
            status = FileStatus::Added;
            continue;
        }
        if raw_line.starts_with("deleted file mode ") {
            status = FileStatus::Deleted;
            continue;
        }
        if let Some(path) = raw_line.strip_prefix("rename from ") {
            old_path = Some(path.into());
            status = FileStatus::Renamed;
            continue;
        }
        if raw_line.starts_with("rename to ") {
            status = FileStatus::Renamed;
            continue;
        }
        if let Some(path) = raw_line.strip_prefix("--- ") {
            old_path = Some(path.strip_prefix("a/").unwrap_or(path).to_string());
            continue;
        }
        if let Some(path) = raw_line.strip_prefix("+++ ") {
            let path = if path == "/dev/null" {
                old_path.clone().unwrap_or_else(|| path.into())
            } else {
                path.strip_prefix("b/").unwrap_or(path).into()
            };
            current = Some(FileDiff {
                path,
                old_path: old_path.clone(),
                status,
                lines: Vec::new(),
            });
            continue;
        }
        let Some(file) = current.as_mut() else {
            continue;
        };
        if let Some(c) = hunk.captures(raw_line) {
            old_no = c[1].parse().unwrap_or(0);
            new_no = c[3].parse().unwrap_or(0);
            file.lines.push(DiffLine {
                text: raw_line.into(),
                kind: LineKind::Hunk,
                old: None,
                new: None,
                syntax: Vec::new(),
            });
        } else if raw_line.starts_with('+') && !raw_line.starts_with("+++") {
            file.lines.push(DiffLine {
                text: raw_line[1..].into(),
                kind: LineKind::Add,
                old: None,
                new: Some(new_no),
                syntax: Vec::new(),
            });
            new_no += 1;
        } else if raw_line.starts_with('-') && !raw_line.starts_with("---") {
            file.lines.push(DiffLine {
                text: raw_line[1..].into(),
                kind: LineKind::Remove,
                old: Some(old_no),
                new: None,
                syntax: Vec::new(),
            });
            old_no += 1;
        } else if let Some(text) = raw_line.strip_prefix(' ') {
            file.lines.push(DiffLine {
                text: text.into(),
                kind: LineKind::Context,
                old: Some(old_no),
                new: Some(new_no),
                syntax: Vec::new(),
            });
            old_no += 1;
            new_no += 1;
        } else if raw_line.starts_with("\\ No newline") {
            file.lines.push(DiffLine {
                text: raw_line.into(),
                kind: LineKind::Meta,
                old: None,
                new: None,
                syntax: Vec::new(),
            });
        }
    }
    if let Some(file) = current {
        files.push(file);
    }
    for file in &mut files {
        highlight(file);
    }
    files
}

#[cfg(test)]
pub fn file_view_lines(path: &str, contents: &str) -> Vec<DiffLine> {
    let mut file = FileDiff {
        path: path.into(),
        old_path: None,
        status: FileStatus::Modified,
        lines: contents
            .lines()
            .enumerate()
            .map(|(index, text)| DiffLine {
                text: text.into(),
                kind: LineKind::Context,
                old: u32::try_from(index + 1).ok(),
                new: u32::try_from(index + 1).ok(),
                syntax: Vec::new(),
            })
            .collect(),
    };
    if file.lines.is_empty() {
        file.lines.push(DiffLine {
            text: String::new(),
            kind: LineKind::Context,
            old: Some(1),
            new: Some(1),
            syntax: Vec::new(),
        });
    }
    highlight(&mut file);
    file.lines
}

#[cfg(test)]
mod tests {
    use super::*;
    const SAMPLE: &str = "diff --git a/app.py b/app.py\n--- a/app.py\n+++ b/app.py\n@@ -1,2 +1,3 @@\n old\n-bad()\n+good()\n+extra()\n";
    #[test]
    fn parses_both_gutters() {
        let f = parse_unified_diff(SAMPLE);
        assert_eq!(f[0].additions(), 2);
        assert_eq!(f[0].lines[2].old, Some(2));
        assert_eq!(f[0].lines[3].new, Some(2));
    }

    #[test]
    fn parses_hunk_ranges() {
        assert_eq!(
            hunk_ranges("@@ -10,3 +12,5 @@ fn main"),
            Some((10, 12, 12, 16))
        );
    }

    #[test]
    fn file_view_changes_match_editor_style_gutter_markers() {
        let diff = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1,6 +1,6 @@\n unchanged\n-old\n+changed\n unchanged\n+added\n middle\n-removed\n tail\n";
        let files = parse_unified_diff(diff);

        assert_eq!(
            file_view_changes(&files[0]),
            vec![
                (2, FileViewChange::Modified),
                (4, FileViewChange::Added),
                (6, FileViewChange::Deleted),
            ]
        );

        let deletion_at_eof = parse_unified_diff(
            "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1,2 +1 @@\n keep\n-removed\n",
        );
        assert_eq!(
            file_view_changes(&deletion_at_eof[0]),
            vec![(1, FileViewChange::Deleted)]
        );
    }

    fn assert_highlighting_recovers_after_comment(
        path: &str,
        comment: &str,
        code: &str,
        keyword: &str,
    ) {
        let diff = format!(
            "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -0,0 +1,2 @@\n+{comment}\n+{code}\n"
        );
        let files = parse_unified_diff(&diff);
        let added = files[0]
            .lines
            .iter()
            .filter(|line| line.kind == LineKind::Add)
            .collect::<Vec<_>>();
        assert_eq!(added.len(), 2);
        assert_eq!(
            added[1]
                .syntax
                .iter()
                .map(|span| span.text.as_str())
                .collect::<String>(),
            code
        );
        let comment_color = added[0]
            .syntax
            .iter()
            .find(|span| !span.text.trim().is_empty())
            .map(|span| span.rgb)
            .unwrap();
        let keyword_color = added[1]
            .syntax
            .iter()
            .find(|span| span.text.trim() == keyword)
            .unwrap_or_else(|| panic!("{path}: keyword {keyword:?} was not highlighted"))
            .rgb;
        assert_ne!(
            keyword_color, comment_color,
            "{path}: comment scope leaked into the following line"
        );
    }

    #[test]
    fn line_comments_do_not_leak_into_following_code() {
        for (path, comment, code, keyword) in [
            ("app.py", "# comment", "assert value == 'ok'", "assert"),
            ("app.rs", "// comment", "let value = \"ok\";", "let"),
            ("app.js", "// comment", "const value = 'ok';", "const"),
            ("app.rb", "# comment", "def value", "def"),
        ] {
            assert_highlighting_recovers_after_comment(path, comment, code, keyword);
        }
    }

    #[test]
    fn block_comment_closes_before_following_c_code() {
        let path = "app.c";
        let diff = format!(
            "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -0,0 +1,3 @@\n+/* comment\n+still comment */\n+int value = 1;\n"
        );
        let files = parse_unified_diff(&diff);
        let added = files[0]
            .lines
            .iter()
            .filter(|line| line.kind == LineKind::Add)
            .collect::<Vec<_>>();
        let comment_color = added[1]
            .syntax
            .iter()
            .find(|span| span.text.contains("still"))
            .unwrap()
            .rgb;
        let keyword_color = added[2]
            .syntax
            .iter()
            .find(|span| span.text.trim() == "int")
            .unwrap()
            .rgb;
        assert_ne!(keyword_color, comment_color);
        assert_eq!(
            added[2]
                .syntax
                .iter()
                .map(|span| span.text.as_str())
                .collect::<String>(),
            "int value = 1;"
        );
    }
}
