use crate::config::DEFAULT_MAX_TEXT_BYTES;
use anyhow::{Context, Result};
use ignore::WalkBuilder;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

/// A stable, relative-path-indexed view of the watched directory.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Snapshot {
    files: BTreeMap<PathBuf, FileContent>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FileContent {
    Text(String),
    Opaque(FileFingerprint),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileFingerprint {
    pub byte_len: u64,
    pub digest: blake3::Hash,
    pub reason: OpaqueReason,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpaqueReason {
    Binary,
    TooLarge,
}

#[derive(Clone, Copy, Debug)]
pub struct CaptureOptions {
    /// Files larger than this are fingerprinted instead of retained in memory.
    pub max_text_bytes: u64,
}

impl Default for CaptureOptions {
    fn default() -> Self {
        Self {
            max_text_bytes: DEFAULT_MAX_TEXT_BYTES,
        }
    }
}

impl Snapshot {
    /// Reads a complete snapshot. The journal directory is always excluded so
    /// diffwatch cannot observe its own writes.
    pub fn capture(root: &Path, journal_dir: &Path) -> Result<Self> {
        Self::capture_with_options(root, journal_dir, CaptureOptions::default())
    }

    pub fn capture_with_options(
        root: &Path,
        journal_dir: &Path,
        options: CaptureOptions,
    ) -> Result<Self> {
        let mut files = BTreeMap::new();
        let mut walker = WalkBuilder::new(root);
        let walk_root = root.to_path_buf();
        walker
            .hidden(false)
            .git_ignore(true)
            .git_global(false)
            .git_exclude(false)
            // `.gitignore` is useful configuration even when Git itself is
            // absent. This is the normal mode for diffwatch.
            .require_git(false)
            .filter_entry(move |entry| {
                entry
                    .path()
                    .strip_prefix(&walk_root)
                    .ok()
                    .and_then(|relative| relative.components().next())
                    .is_none_or(|component| component.as_os_str() != ".git")
            });

        for entry in walker.build() {
            let entry = entry.context("failed to walk watched directory")?;
            let absolute = entry.path();
            if !entry.file_type().is_some_and(|kind| kind.is_file()) {
                continue;
            }
            if absolute.starts_with(journal_dir) {
                continue;
            }

            let relative = absolute
                .strip_prefix(root)
                .context("walked file was outside watched directory")?
                .to_path_buf();
            let content = read_content(absolute, options)?;
            files.insert(relative, content);
        }

        Ok(Self { files })
    }

    pub fn changes_from(&self, previous: &Self) -> Vec<FileChange> {
        let paths: BTreeSet<_> = previous.files.keys().chain(self.files.keys()).collect();
        paths
            .into_iter()
            .filter_map(
                |path| match (previous.files.get(path), self.files.get(path)) {
                    (None, Some(after)) => Some(FileChange::Added {
                        path: path.clone(),
                        after: after.clone(),
                    }),
                    (Some(before), None) => Some(FileChange::Deleted {
                        path: path.clone(),
                        before: before.clone(),
                    }),
                    (Some(before), Some(after)) if before != after => Some(FileChange::Modified {
                        path: path.clone(),
                        before: before.clone(),
                        after: after.clone(),
                    }),
                    _ => None,
                },
            )
            .collect()
    }
}

fn read_content(path: &Path, options: CaptureOptions) -> Result<FileContent> {
    let byte_len = fs::metadata(path)
        .with_context(|| format!("failed to inspect {}", path.display()))?
        .len();
    if byte_len > options.max_text_bytes {
        return fingerprint(path, byte_len, OpaqueReason::TooLarge);
    }

    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    match String::from_utf8(bytes) {
        Ok(text) => Ok(FileContent::Text(text)),
        Err(error) => Ok(FileContent::Opaque(FileFingerprint {
            byte_len,
            digest: blake3::hash(error.as_bytes()),
            reason: OpaqueReason::Binary,
        })),
    }
}

fn fingerprint(path: &Path, byte_len: u64, reason: OpaqueReason) -> Result<FileContent> {
    let mut file =
        fs::File::open(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .with_context(|| format!("failed to read {}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(FileContent::Opaque(FileFingerprint {
        byte_len,
        digest: hasher.finalize(),
        reason,
    }))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FileChange {
    Added {
        path: PathBuf,
        after: FileContent,
    },
    Deleted {
        path: PathBuf,
        before: FileContent,
    },
    Modified {
        path: PathBuf,
        before: FileContent,
        after: FileContent,
    },
}

impl FileChange {
    pub fn path(&self) -> &Path {
        match self {
            Self::Added { path, .. } | Self::Deleted { path, .. } | Self::Modified { path, .. } => {
                path
            }
        }
    }

    pub fn kind(&self) -> ChangeKind {
        match self {
            Self::Added { .. } => ChangeKind::Added,
            Self::Deleted { .. } => ChangeKind::Deleted,
            Self::Modified { .. } => ChangeKind::Modified,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChangeKind {
    Added,
    Deleted,
    Modified,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn text(value: &str) -> FileContent {
        FileContent::Text(value.to_owned())
    }

    #[test]
    fn detects_added_modified_and_deleted_files_in_path_order() {
        let previous = Snapshot {
            files: BTreeMap::from([
                (PathBuf::from("deleted.txt"), text("old")),
                (PathBuf::from("modified.txt"), text("before")),
            ]),
        };
        let current = Snapshot {
            files: BTreeMap::from([
                (PathBuf::from("added.txt"), text("new")),
                (PathBuf::from("modified.txt"), text("after")),
            ]),
        };

        let changes = current.changes_from(&previous);

        assert_eq!(changes.len(), 3);
        assert!(matches!(changes[0], FileChange::Added { .. }));
        assert!(matches!(changes[1], FileChange::Deleted { .. }));
        assert!(matches!(changes[2], FileChange::Modified { .. }));
    }

    #[test]
    fn capture_never_includes_its_own_journal() {
        let directory = tempdir().unwrap();
        let journal = directory.path().join(".diffwatch");
        fs::create_dir(&journal).unwrap();
        fs::write(directory.path().join("source.txt"), "source").unwrap();
        fs::write(journal.join("0001.diff"), "journal").unwrap();

        let snapshot = Snapshot::capture(directory.path(), &journal).unwrap();

        assert_eq!(snapshot.files.len(), 1);
        assert!(snapshot.files.contains_key(Path::new("source.txt")));
    }

    #[test]
    fn large_files_are_fingerprinted_and_changes_are_detected() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("large.txt");
        fs::write(&path, "first version").unwrap();
        let options = CaptureOptions { max_text_bytes: 4 };
        let before = Snapshot::capture_with_options(
            directory.path(),
            &directory.path().join("journal"),
            options,
        )
        .unwrap();

        fs::write(&path, "other version").unwrap();
        let after = Snapshot::capture_with_options(
            directory.path(),
            &directory.path().join("journal"),
            options,
        )
        .unwrap();

        assert_eq!(after.changes_from(&before).len(), 1);
        assert!(matches!(
            after.files.get(Path::new("large.txt")),
            Some(FileContent::Opaque(FileFingerprint {
                reason: OpaqueReason::TooLarge,
                ..
            }))
        ));
    }

    #[test]
    fn capture_honors_gitignore_outside_git_repositories() {
        let directory = tempdir().unwrap();
        fs::write(directory.path().join(".gitignore"), "/target/\n").unwrap();
        fs::create_dir(directory.path().join("target")).unwrap();
        fs::write(directory.path().join("target/output"), "ignored").unwrap();

        let snapshot =
            Snapshot::capture(directory.path(), &directory.path().join("journal")).unwrap();

        assert!(!snapshot.files.contains_key(Path::new("target/output")));
        assert!(snapshot.files.contains_key(Path::new(".gitignore")));
    }

    #[test]
    fn capture_never_includes_git_metadata() {
        let directory = tempdir().unwrap();
        fs::create_dir(directory.path().join(".git")).unwrap();
        fs::write(directory.path().join(".git/index"), "metadata").unwrap();
        fs::write(directory.path().join(".gitignore"), "").unwrap();

        let snapshot =
            Snapshot::capture(directory.path(), &directory.path().join("journal")).unwrap();

        assert!(!snapshot.files.contains_key(Path::new(".git/index")));
        assert!(snapshot.files.contains_key(Path::new(".gitignore")));
    }
}
