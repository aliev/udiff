use crate::batch::Batch;
use crate::snapshot::ChangeKind;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

/// Append-only on-disk representation of one watcher run.
pub struct Journal {
    directory: PathBuf,
}

impl Journal {
    pub fn create(root: &Path, started_at: DateTime<Utc>, settle_seconds: f64) -> Result<Self> {
        let directory = unique_session_directory(root, started_at);
        fs::create_dir_all(&directory)
            .with_context(|| format!("cannot create journal at {}", directory.display()))?;

        let metadata = SessionMetadata {
            version: 1,
            started_at,
            settle_seconds,
        };
        write_json(&directory.join("session.json"), &metadata)?;
        Ok(Self { directory })
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn write_batch(
        &self,
        batch: &Batch,
        started_at: DateTime<Utc>,
        finished_at: DateTime<Utc>,
    ) -> Result<()> {
        let stem = format!("{:04}", batch.number);
        let diff_path = self.directory.join(format!("{stem}.diff"));
        fs::write(&diff_path, batch.render())
            .with_context(|| format!("cannot write {}", diff_path.display()))?;

        let files = batch
            .changes
            .iter()
            .map(|change| ChangedFile {
                path: change.path().to_string_lossy().into_owned(),
                change: match change.kind() {
                    ChangeKind::Added => "added",
                    ChangeKind::Deleted => "deleted",
                    ChangeKind::Modified => "modified",
                },
            })
            .collect();
        let metadata = BatchMetadata {
            number: batch.number,
            started_at,
            finished_at,
            files,
        };
        write_json(&self.directory.join(format!("{stem}.json")), &metadata)
    }
}

fn unique_session_directory(root: &Path, started_at: DateTime<Utc>) -> PathBuf {
    let base = started_at.format("session-%Y%m%dT%H%M%S%.3fZ").to_string();
    let candidate = root.join(&base);
    if !candidate.exists() {
        return candidate;
    }

    for suffix in 2_u32.. {
        let candidate = root.join(format!("{base}-{suffix}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!("session suffix space is infinite")
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut encoded =
        serde_json::to_vec_pretty(value).context("cannot serialize journal metadata")?;
    encoded.push(b'\n');
    fs::write(path, encoded).with_context(|| format!("cannot write {}", path.display()))
}

#[derive(Serialize)]
struct SessionMetadata {
    version: u8,
    started_at: DateTime<Utc>,
    settle_seconds: f64,
}

#[derive(Serialize)]
struct BatchMetadata {
    number: u64,
    started_at: DateTime<Utc>,
    finished_at: DateTime<Utc>,
    files: Vec<ChangedFile>,
}

#[derive(Serialize)]
struct ChangedFile {
    path: String,
    change: &'static str,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::{FileChange, FileContent};
    use tempfile::tempdir;

    #[test]
    fn writes_diff_and_machine_readable_metadata() {
        let directory = tempdir().unwrap();
        let now = Utc::now();
        let journal = Journal::create(directory.path(), now, 2.0).unwrap();
        let batch = Batch {
            number: 1,
            changes: vec![FileChange::Added {
                path: PathBuf::from("new.txt"),
                after: FileContent::Text("hello\n".into()),
            }],
        };

        journal.write_batch(&batch, now, now).unwrap();

        assert!(journal.directory().join("0001.diff").is_file());
        let metadata = fs::read_to_string(journal.directory().join("0001.json")).unwrap();
        assert!(metadata.contains("\"path\": \"new.txt\""));
        assert!(metadata.contains("\"change\": \"added\""));
    }
}
