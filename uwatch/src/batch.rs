use crate::snapshot::{FileChange, FileContent, OpaqueReason, Snapshot};
use similar::TextDiff;
use std::fmt::Write;

#[derive(Debug)]
pub struct Batch {
    pub number: u64,
    pub changes: Vec<FileChange>,
}

impl Batch {
    pub fn between(number: u64, previous: &Snapshot, current: &Snapshot) -> Self {
        Self {
            number,
            changes: current.changes_from(previous),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    pub fn render(&self) -> String {
        let mut output = format!("Batch #{}\n", self.number);
        output.push_str(&self.render_unified_diff());
        output
    }

    /// Renders the batch as a standard multi-file unified diff.
    pub fn render_unified_diff(&self) -> String {
        let mut output = String::new();
        for change in &self.changes {
            render_change(&mut output, change);
        }
        output
    }
}

fn render_change(output: &mut String, change: &FileChange) {
    match change {
        FileChange::Added { path, after } => {
            let name = path.display().to_string();
            let _ = writeln!(output, "diff --git a/{name} b/{name}");
            let _ = writeln!(output, "new file mode 100644");
            render_content_diff(output, "/dev/null", &format!("b/{name}"), None, Some(after));
        }
        FileChange::Deleted { path, before } => {
            let name = path.display().to_string();
            let _ = writeln!(output, "diff --git a/{name} b/{name}");
            let _ = writeln!(output, "deleted file mode 100644");
            render_content_diff(
                output,
                &format!("a/{name}"),
                "/dev/null",
                Some(before),
                None,
            );
        }
        FileChange::Modified {
            path,
            before,
            after,
        } => {
            let name = path.display().to_string();
            let _ = writeln!(output, "diff --git a/{name} b/{name}");
            render_content_diff(
                output,
                &format!("a/{name}"),
                &format!("b/{name}"),
                Some(before),
                Some(after),
            );
        }
    }
}

fn render_content_diff(
    output: &mut String,
    old_name: &str,
    new_name: &str,
    before: Option<&FileContent>,
    after: Option<&FileContent>,
) {
    match (before, after) {
        (Some(FileContent::Opaque(fingerprint)), _)
        | (_, Some(FileContent::Opaque(fingerprint))) => {
            let description = match fingerprint.reason {
                OpaqueReason::Binary => "Binary file",
                OpaqueReason::TooLarge => "Large file",
            };
            let _ = writeln!(
                output,
                "{description} {old_name} -> {new_name} differs ({} bytes)",
                fingerprint.byte_len
            );
        }
        _ => {
            let before = match before {
                Some(FileContent::Text(text)) => text.as_str(),
                _ => "",
            };
            let after = match after {
                Some(FileContent::Text(text)) => text.as_str(),
                _ => "",
            };
            let diff = TextDiff::from_lines(before, after)
                .unified_diff()
                .context_radius(3)
                .header(old_name, new_name)
                .to_string();
            output.push_str(&diff);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn renders_unified_diff() {
        let batch = Batch {
            number: 7,
            changes: vec![FileChange::Modified {
                path: PathBuf::from("hello.txt"),
                before: FileContent::Text("hello\n".into()),
                after: FileContent::Text("hello world\n".into()),
            }],
        };

        let rendered = batch.render();
        assert!(rendered.contains("Batch #7"));
        assert!(rendered.contains("diff --git a/hello.txt b/hello.txt"));
        assert!(rendered.contains("--- a/hello.txt"));
        assert!(rendered.contains("+hello world"));
    }

    #[test]
    fn separates_files_for_multi_file_diff_parsers() {
        let batch = Batch {
            number: 1,
            changes: vec![
                FileChange::Added {
                    path: PathBuf::from("first.txt"),
                    after: FileContent::Text("first\n".into()),
                },
                FileChange::Added {
                    path: PathBuf::from("second.txt"),
                    after: FileContent::Text("second\n".into()),
                },
            ],
        };

        let rendered = batch.render_unified_diff();

        assert_eq!(rendered.matches("diff --git ").count(), 2);
        assert!(rendered.contains("+++ b/first.txt"));
        assert!(rendered.contains("+++ b/second.txt"));
    }
}
