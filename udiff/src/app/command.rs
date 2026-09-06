use crate::model::FileDiff;
use crossterm::event::{KeyEvent, MouseEvent};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorTarget {
    pub path: String,
    /// New-side line to open at. The file on disk is the new side, so the old
    /// numbers would land in the wrong place.
    pub line: Option<u32>,
}

#[cfg_attr(not(feature = "watch"), allow(dead_code))]
pub enum Command {
    Key(KeyEvent),
    Mouse(MouseEvent),
    RevisionReceived { number: u64, files: Vec<FileDiff> },
    WatchStopped,
    WatchError(String),
}

#[derive(Debug, Eq, PartialEq)]
pub enum Effect {
    None,
    Quit,
    ResetWatch,
    Copy(String),
    OpenEditor(EditorTarget),
}
