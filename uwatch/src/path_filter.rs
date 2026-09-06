use anyhow::{Context, Result};
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use std::path::{Path, PathBuf};

/// Applies the same exclusions to filesystem events that snapshot capture
/// applies while walking the directory.
pub struct PathFilter {
    root: PathBuf,
    journal: PathBuf,
    ignore: Gitignore,
}

impl PathFilter {
    pub fn new(root: &Path, journal: &Path) -> Result<Self> {
        let mut builder = GitignoreBuilder::new(root);
        let ignore_file = root.join(".gitignore");
        if ignore_file.is_file() {
            if let Some(error) = builder.add(&ignore_file) {
                return Err(error).context("cannot read .gitignore");
            }
        }
        let ignore = builder.build().context("cannot parse .gitignore")?;
        Ok(Self {
            root: root.to_path_buf(),
            journal: journal.to_path_buf(),
            ignore,
        })
    }

    pub fn includes(&self, path: &Path) -> bool {
        if path.starts_with(&self.journal) {
            return false;
        }
        let relative = path.strip_prefix(&self.root).unwrap_or(path);
        if is_git_metadata(relative) {
            return false;
        }
        !self
            .ignore
            .matched_path_or_any_parents(relative, path.is_dir())
            .is_ignore()
    }

    pub fn includes_any<'a>(&self, paths: impl IntoIterator<Item = &'a PathBuf>) -> bool {
        paths.into_iter().any(|path| self.includes(path))
    }
}

fn is_git_metadata(relative: &Path) -> bool {
    relative
        .components()
        .next()
        .is_some_and(|component| component.as_os_str() == ".git")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn honors_gitignore_without_a_git_repository() {
        let directory = tempdir().unwrap();
        fs::write(directory.path().join(".gitignore"), "/target/\n").unwrap();
        let target = directory.path().join("target");
        let source = directory.path().join("src/main.rs");
        let filter = PathFilter::new(directory.path(), &directory.path().join(".uwatch")).unwrap();

        assert!(!filter.includes(&target.join("debug/output")));
        assert!(filter.includes(&source));
    }

    #[test]
    fn always_excludes_journal() {
        let directory = tempdir().unwrap();
        let journal = directory.path().join("custom-journal");
        let filter = PathFilter::new(directory.path(), &journal).unwrap();

        assert!(!filter.includes(&journal.join("session/0001.diff")));
    }

    #[test]
    fn always_excludes_git_metadata_but_not_similarly_named_files() {
        let directory = tempdir().unwrap();
        let filter = PathFilter::new(directory.path(), &directory.path().join(".uwatch")).unwrap();

        assert!(!filter.includes(&directory.path().join(".git/index")));
        assert!(!filter.includes(&directory.path().join(".git/refs/heads/main")));
        assert!(filter.includes(&directory.path().join(".github/workflows/test.yml")));
        assert!(filter.includes(&directory.path().join(".gitignore")));
    }
}
