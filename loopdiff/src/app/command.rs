use crate::model::{DiffLine, FileDiff};
use crossterm::event::{KeyEvent, MouseEvent};

pub enum Command {
    Key(KeyEvent),
    Mouse(MouseEvent),
    FileViewLoaded {
        file: usize,
        lines: Option<Vec<DiffLine>>,
    },
    BatchReceived {
        number: u64,
        files: Vec<FileDiff>,
    },
    WatchStopped,
    WatchError(String),
}

#[derive(Debug, Eq, PartialEq)]
pub enum Effect {
    None,
    Quit,
    ResetWatch,
    Copy(String),
    RequestFileView(usize),
    OpenFile(String),
}
