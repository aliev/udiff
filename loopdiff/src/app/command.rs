use crate::model::FileDiff;
use crossterm::event::{KeyEvent, MouseEvent};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorTarget {
    pub path: String,
}

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
