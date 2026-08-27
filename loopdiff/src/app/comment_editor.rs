use super::editor::{next_boundary, previous_boundary, vertical_cursor};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    None,
    Save,
    Cancel,
}

#[derive(Default)]
pub struct CommentEditor {
    pub text: String,
    pub cursor: usize,
    pub anchor: Option<usize>,
    pub editing_key: Option<String>,
}

impl CommentEditor {
    pub fn open(&mut self, text: String, anchor: usize, editing_key: Option<String>) {
        self.cursor = text.len();
        self.text = text;
        self.anchor = Some(anchor);
        self.editing_key = editing_key;
    }

    pub fn close(&mut self) {
        self.text.clear();
        self.cursor = 0;
        self.anchor = None;
        self.editing_key = None;
    }

    pub fn event(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Esc => return Action::Cancel,
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.text.insert(self.cursor, '\n');
                self.cursor += 1;
            }
            KeyCode::Enter => return Action::Save,
            KeyCode::Char('j') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.text.insert(self.cursor, '\n');
                self.cursor += 1;
            }
            KeyCode::Backspace => {
                if let Some(previous) = previous_boundary(&self.text, self.cursor) {
                    self.text.drain(previous..self.cursor);
                    self.cursor = previous;
                }
            }
            KeyCode::Left => {
                if let Some(previous) = previous_boundary(&self.text, self.cursor) {
                    self.cursor = previous;
                }
            }
            KeyCode::Right => {
                if let Some(next) = next_boundary(&self.text, self.cursor) {
                    self.cursor = next;
                }
            }
            KeyCode::Up => self.cursor = vertical_cursor(&self.text, self.cursor, false),
            KeyCode::Down => self.cursor = vertical_cursor(&self.text, self.cursor, true),
            KeyCode::Char(character)
                if !key.modifiers.intersects(
                    KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER,
                ) =>
            {
                self.text.insert(self.cursor, character);
                self.cursor += character.len_utf8();
            }
            _ => {}
        }
        Action::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_owns_utf8_safe_input_and_reports_intent() {
        let mut editor = CommentEditor::default();
        editor.open("é".into(), 3, None);
        assert_eq!(
            editor.event(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)),
            Action::None
        );
        assert_eq!(editor.cursor, 0);
        editor.event(KeyEvent::new(KeyCode::Char('ζ'), KeyModifiers::NONE));
        assert_eq!(editor.text, "ζé");
        assert_eq!(
            editor.event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Action::Save
        );
        assert_eq!(editor.anchor, Some(3));
    }
}
