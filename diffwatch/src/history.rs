use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Eq, PartialEq)]
pub struct SessionSummary {
    pub name: String,
    pub directory: PathBuf,
    pub batch_count: usize,
}

pub fn list_sessions(journal_root: &Path) -> Result<Vec<SessionSummary>> {
    if !journal_root.exists() {
        return Ok(Vec::new());
    }

    let mut sessions = Vec::new();
    for entry in fs::read_dir(journal_root)
        .with_context(|| format!("cannot read {}", journal_root.display()))?
    {
        let entry = entry.context("cannot read journal entry")?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with("session-") {
            continue;
        }
        let batch_count = count_diffs(&entry.path())?;
        sessions.push(SessionSummary {
            name,
            directory: entry.path(),
            batch_count,
        });
    }
    sessions.sort_by(|left, right| right.name.cmp(&left.name));
    Ok(sessions)
}

pub fn read_batch(journal_root: &Path, session: Option<&str>, number: u64) -> Result<String> {
    let directory = match session {
        Some(name) => journal_root.join(name),
        None => {
            list_sessions(journal_root)?
                .into_iter()
                .next()
                .context("no diffwatch sessions found")?
                .directory
        }
    };
    let path = directory.join(format!("{number:04}.diff"));
    fs::read_to_string(&path).with_context(|| format!("cannot read {}", path.display()))
}

fn count_diffs(directory: &Path) -> Result<usize> {
    let mut count = 0;
    for entry in
        fs::read_dir(directory).with_context(|| format!("cannot read {}", directory.display()))?
    {
        let entry = entry.context("cannot read session entry")?;
        if entry
            .path()
            .extension()
            .is_some_and(|extension| extension == "diff")
        {
            count += 1;
        }
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn lists_newest_session_first_and_counts_batches() {
        let directory = tempdir().unwrap();
        let older = directory.path().join("session-20260101T000000.000Z");
        let newer = directory.path().join("session-20260102T000000.000Z");
        fs::create_dir(&older).unwrap();
        fs::create_dir(&newer).unwrap();
        fs::write(older.join("0001.diff"), "one").unwrap();
        fs::write(newer.join("0001.diff"), "one").unwrap();
        fs::write(newer.join("0002.diff"), "two").unwrap();

        let sessions = list_sessions(directory.path()).unwrap();

        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].name, "session-20260102T000000.000Z");
        assert_eq!(sessions[0].batch_count, 2);
    }

    #[test]
    fn reads_batch_from_latest_session_by_default() {
        let directory = tempdir().unwrap();
        let session = directory.path().join("session-20260101T000000.000Z");
        fs::create_dir(&session).unwrap();
        fs::write(session.join("0003.diff"), "the diff").unwrap();

        assert_eq!(read_batch(directory.path(), None, 3).unwrap(), "the diff");
    }
}
