use anyhow::{Context, Result};
use std::io::{self, IsTerminal, Read};

pub trait DiffSource {
    fn read(&mut self) -> Result<String>;
}

pub struct StdinDiffSource;

impl StdinDiffSource {
    #[cfg(feature = "watch")]
    pub fn read_optional(&mut self) -> Result<Option<String>> {
        let mut stdin = io::stdin();
        let is_terminal = stdin.is_terminal();
        read_optional_diff(&mut stdin, is_terminal)
    }
}

#[cfg(feature = "watch")]
mod watch {
    use crate::model::{FileDiff, parse_unified_diff};
    use anyhow::Result;
    use std::path::PathBuf;
    use std::sync::mpsc::TryRecvError;
    use uwatch::watcher::{WatchEvent, WatchHandle, WatchOptions};

    #[derive(Debug)]
    pub enum WatchInputEvent {
        Batch { number: u64, files: Vec<FileDiff> },
        Error(String),
        Closed,
    }

    pub struct WatchSource {
        handle: WatchHandle,
        root: PathBuf,
    }

    impl WatchSource {
        pub fn start(root: PathBuf) -> Result<Self> {
            let root = if root.is_absolute() {
                root
            } else {
                std::env::current_dir()?.join(root)
            };
            Ok(Self {
                handle: WatchHandle::start(WatchOptions::new(root.clone()))?,
                root,
            })
        }

        pub fn root(&self) -> &std::path::Path {
            &self.root
        }

        pub fn try_recv(&self) -> std::result::Result<WatchInputEvent, TryRecvError> {
            self.handle.try_recv().map(watch_input_event)
        }
    }

    fn watch_input_event(event: WatchEvent) -> WatchInputEvent {
        match event {
            WatchEvent::Batch {
                number,
                started_at: _,
                finished_at: _,
                unified_diff,
            } => {
                let files = parse_unified_diff(&unified_diff);
                if files.is_empty() {
                    WatchInputEvent::Error(format!("revision {number} contains no supported diff"))
                } else {
                    WatchInputEvent::Batch { number, files }
                }
            }
            WatchEvent::Warning(error) => WatchInputEvent::Error(error),
            WatchEvent::Stopped => WatchInputEvent::Closed,
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

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
}

#[cfg(feature = "watch")]
pub use watch::{WatchInputEvent, WatchSource};

impl DiffSource for StdinDiffSource {
    fn read(&mut self) -> Result<String> {
        let mut stdin = io::stdin();
        let is_terminal = stdin.is_terminal();
        read_diff(&mut stdin, is_terminal)
    }
}

fn read_diff(reader: &mut impl Read, is_terminal: bool) -> Result<String> {
    if is_terminal {
        anyhow::bail!("expected a unified diff on stdin (for example: git diff | udiff)");
    }
    let mut raw = String::new();
    reader
        .read_to_string(&mut raw)
        .context("could not read diff from stdin")?;
    Ok(raw)
}

#[cfg(feature = "watch")]
fn read_optional_diff(reader: &mut impl Read, is_terminal: bool) -> Result<Option<String>> {
    if is_terminal {
        return Ok(None);
    }
    read_diff(reader, false).map(Some)
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

    #[cfg(feature = "watch")]
    #[test]
    fn optional_reader_distinguishes_a_terminal_from_piped_context() {
        let mut terminal = Cursor::new(Vec::new());
        assert_eq!(read_optional_diff(&mut terminal, true).unwrap(), None);

        let mut pipe = Cursor::new(b"initial diff".to_vec());
        assert_eq!(
            read_optional_diff(&mut pipe, false).unwrap(),
            Some("initial diff".into())
        );
    }
}
