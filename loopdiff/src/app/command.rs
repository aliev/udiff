use crate::model::{DiffLine, FileDiff};
use crossterm::event::{KeyEvent, MouseEvent};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorTarget {
    pub path: String,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub capture_changes: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RevisionOrigin {
    Context,
    Observed,
    Human(String),
}

impl RevisionOrigin {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Context => "context",
            Self::Observed => "observed",
            Self::Human(name) => name,
        }
    }

    pub fn is_human_edit(&self) -> bool {
        matches!(self, Self::Human(_))
    }
}

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
        origin: RevisionOrigin,
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
    OpenEditor(EditorTarget),
}

#[cfg(test)]
mod tests {
    use super::RevisionOrigin;

    #[test]
    fn revision_origins_have_stable_ui_names() {
        assert_eq!(RevisionOrigin::Observed.as_str(), "observed");
        assert_eq!(RevisionOrigin::Context.as_str(), "context");
        assert_eq!(RevisionOrigin::Human("Ada".into()).as_str(), "Ada");
    }

    #[test]
    fn revision_origin_identifies_human_edits() {
        assert!(!RevisionOrigin::Observed.is_human_edit());
        assert!(RevisionOrigin::Human("Ada".into()).is_human_edit());
    }
}
