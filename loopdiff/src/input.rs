use crate::model::{FileDiff, parse_unified_diff};
use anyhow::{Context, Result};
use diffwatch::watcher::{WatchEvent, WatchHandle, WatchOptions};
use std::io::{self, IsTerminal, Read};
use std::path::PathBuf;
use std::sync::mpsc::TryRecvError;

pub trait DiffSource {
    fn read(&mut self) -> Result<String>;
}

pub struct StdinDiffSource;

#[derive(Debug)]
pub enum WatchInputEvent {
    Batch { number: u64, files: Vec<FileDiff> },
    Error(String),
    Closed,
}

pub struct WatchSource {
    handle: WatchHandle,
}

impl WatchSource {
    pub fn start(root: PathBuf) -> Result<Self> {
        Ok(Self {
            handle: WatchHandle::start(WatchOptions::new(root))?,
        })
    }

    pub fn try_recv(&self) -> std::result::Result<WatchInputEvent, TryRecvError> {
        self.handle.try_recv().map(watch_input_event)
    }
}

fn watch_input_event(event: WatchEvent) -> WatchInputEvent {
    match event {
        WatchEvent::Batch {
            number,
            unified_diff,
            ..
        } => {
            let files = parse_unified_diff(&unified_diff);
            if files.is_empty() {
                WatchInputEvent::Error(format!("batch {number} contains no supported diff"))
            } else {
                WatchInputEvent::Batch { number, files }
            }
        }
        WatchEvent::Warning(error) => WatchInputEvent::Error(error),
        WatchEvent::Stopped => WatchInputEvent::Closed,
    }
}

impl DiffSource for StdinDiffSource {
    fn read(&mut self) -> Result<String> {
        let mut stdin = io::stdin();
        let is_terminal = stdin.is_terminal();
        read_diff(&mut stdin, is_terminal)
    }
}

fn read_diff(reader: &mut impl Read, is_terminal: bool) -> Result<String> {
    if is_terminal {
        anyhow::bail!("expected a unified diff on stdin (for example: git diff | loopdiff)");
    }
    let mut raw = String::new();
    reader
        .read_to_string(&mut raw)
        .context("could not read diff from stdin")?;
    Ok(raw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn reader_preserves_piped_diff_and_rejects_terminal_input() {
        let mut pipe = Cursor::new(b"diff --git a/a b/a\n".to_vec());
        assert_eq!(read_diff(&mut pipe, false).unwrap(), "diff --git a/a b/a\n");

        let mut terminal = Cursor::new(Vec::new());
        assert!(read_diff(&mut terminal, true).is_err());
    }

    #[test]
    fn watcher_warnings_and_shutdown_map_to_input_events() {
        assert!(matches!(
            watch_input_event(WatchEvent::Warning("overflow".into())),
            WatchInputEvent::Error(error) if error == "overflow"
        ));
        assert!(matches!(
            watch_input_event(WatchEvent::Stopped),
            WatchInputEvent::Closed
        ));
    }
}
