use super::editor::{line_bounds, next_boundary, previous_boundary, vertical_cursor};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    None,
    Save,
    Cancel,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Mode {
    #[default]
    Comment,
    Suggestion,
}

#[derive(Default)]
pub struct CommentEditor {
    pub text: String,
    pub cursor: usize,
    pub anchor: Option<usize>,
    pub editing_key: Option<String>,
    pub mode: Mode,
}

impl CommentEditor {
    pub fn open(&mut self, text: String, anchor: usize, editing_key: Option<String>, mode: Mode) {
        self.cursor = text.len();
        self.text = text;
        self.anchor = Some(anchor);
        self.editing_key = editing_key;
        self.mode = mode;
    }

    pub fn close(&mut self) {
        self.text.clear();
        self.cursor = 0;
        self.anchor = None;
        self.editing_key = None;
        self.mode = Mode::Comment;
    }

    pub fn event(&mut self, key: KeyEvent) -> Action {
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
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
            KeyCode::Char('a') if control => self.cursor = line_bounds(&self.text, self.cursor).0,
            KeyCode::Home => self.cursor = line_bounds(&self.text, self.cursor).0,
            KeyCode::Char('e') if control => self.cursor = line_bounds(&self.text, self.cursor).1,
            KeyCode::End => self.cursor = line_bounds(&self.text, self.cursor).1,
            KeyCode::Char('k') if control => {
                let (_, line_end) = line_bounds(&self.text, self.cursor);
                // Already at the break: there is nothing left on this line to
                // kill but the break itself, so the next line joins this one.
                let end = if line_end == self.cursor {
                    next_boundary(&self.text, self.cursor).unwrap_or(line_end)
                } else {
                    line_end
                };
                self.text.drain(self.cursor..end);
            }
            KeyCode::Char('u') if control => {
                let (line_start, _) = line_bounds(&self.text, self.cursor);
                self.text.drain(line_start..self.cursor);
                self.cursor = line_start;
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

    fn editing(text: &str, cursor: usize) -> CommentEditor {
        let mut editor = CommentEditor::default();
        editor.open(text.into(), 0, None, Mode::Comment);
        editor.cursor = cursor;
        editor
    }

    fn control(editor: &mut CommentEditor, character: char) {
        editor.event(KeyEvent::new(
            KeyCode::Char(character),
            KeyModifiers::CONTROL,
        ));
    }

    #[test]
    fn line_motions_stop_at_the_line_the_cursor_is_on() {
        let mut editor = editing("alpha\nβeta\ngamma", 8);
        control(&mut editor, 'a');
        assert_eq!(editor.cursor, 6);
        control(&mut editor, 'e');
        assert_eq!(editor.cursor, 11, "not the end of the whole text");

        editor.cursor = 8;
        editor.event(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE));
        assert_eq!(editor.cursor, 6);
        editor.event(KeyEvent::new(KeyCode::End, KeyModifiers::NONE));
        assert_eq!(editor.cursor, 11);
    }

    #[test]
    fn killing_forward_and_back_leaves_the_other_lines_alone() {
        let mut editor = editing("alpha\nβeta\ngamma", 8);
        control(&mut editor, 'k');
        assert_eq!(editor.text, "alpha\nβ\ngamma");
        assert_eq!(editor.cursor, 8);

        let mut editor = editing("alpha\nβeta\ngamma", 8);
        control(&mut editor, 'u');
        assert_eq!(editor.text, "alpha\neta\ngamma");
        assert_eq!(editor.cursor, 6);
    }

    #[test]
    fn killing_at_the_end_of_a_line_pulls_the_next_one_up() {
        let mut editor = editing("alpha\nβeta", 11);
        control(&mut editor, 'k');
        assert_eq!(
            editor.text, "alpha\nβeta",
            "nothing follows, so nothing dies"
        );

        let mut editor = editing("alpha\nβeta", 5);
        control(&mut editor, 'k');
        assert_eq!(editor.text, "alphaβeta");
        assert_eq!(editor.cursor, 5);
    }

    #[test]
    fn a_line_can_be_cleared_and_retyped() {
        // The reason these exist: replacing a prefilled suggestion should not
        // take thirty presses of Backspace.
        let mut editor = editing(
            "            .highlight_style(highlight_style(view.focused))",
            59,
        );
        control(&mut editor, 'a');
        control(&mut editor, 'k');
        assert_eq!(editor.text, "");
        assert_eq!(editor.cursor, 0);
    }

    #[test]
    fn editor_owns_utf8_safe_input_and_reports_intent() {
        let mut editor = CommentEditor::default();
        editor.open("é".into(), 3, None, Mode::Comment);
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
