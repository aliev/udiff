use crate::theme::theme;
use crossterm::event::{Event, KeyCode, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

/// Rows the frame itself takes, so only the remainder holds help text.
const FRAME_ROWS: u16 = 2;

#[derive(Default)]
pub struct Help {
    open: bool,
    scroll: usize,
    /// Largest useful scroll offset, recomputed on every draw because it
    /// depends on the terminal height.
    max_scroll: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EventState {
    Consumed,
    Ignored,
}

impl Help {
    pub fn open(&mut self) {
        self.open = true;
        self.scroll = 0;
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn event(&mut self, event: &Event) -> EventState {
        if !self.open {
            return EventState::Ignored;
        }
        let Event::Key(key) = event else {
            return EventState::Consumed;
        };
        let page = (self.max_scroll / 2).max(1);
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') => {
                self.open = false;
                self.scroll = 0;
            }
            KeyCode::Char('j') | KeyCode::Down => self.scroll_by(1),
            KeyCode::Char('k') | KeyCode::Up => self.scroll_by(-1),
            KeyCode::Char('d') if control => self.scroll_by(page as isize),
            KeyCode::Char('u') if control => self.scroll_by(-(page as isize)),
            KeyCode::PageDown => self.scroll_by(page as isize),
            KeyCode::PageUp => self.scroll_by(-(page as isize)),
            KeyCode::Char('g') | KeyCode::Home => self.scroll = 0,
            KeyCode::Char('G') | KeyCode::End => self.scroll = self.max_scroll,
            _ => {}
        }
        EventState::Consumed
    }

    fn scroll_by(&mut self, delta: isize) {
        let target = self.scroll as isize + delta;
        self.scroll = target.clamp(0, self.max_scroll as isize) as usize;
    }

    pub fn draw(&mut self, frame: &mut Frame, root: Rect) {
        if !self.open {
            return;
        }
        let lines = help_lines();
        let width = root.width.saturating_sub(4).min(72);
        let desired_height = u16::try_from(lines.len())
            .unwrap_or(u16::MAX)
            .saturating_add(FRAME_ROWS);
        let height = root.height.saturating_sub(2).min(desired_height);
        let visible = usize::from(height.saturating_sub(FRAME_ROWS));
        self.max_scroll = lines.len().saturating_sub(visible);
        self.scroll = self.scroll.min(self.max_scroll);
        let area = Rect {
            x: root.x + root.width.saturating_sub(width) / 2,
            y: root.y + root.height.saturating_sub(height) / 2,
            width,
            height,
        };
        let footer = if self.max_scroll == 0 {
            " ? or Esc to close ".to_owned()
        } else {
            let last = (self.scroll + visible).min(lines.len());
            format!(
                " {}-{} of {} · j/k scroll · Esc close ",
                self.scroll + 1,
                last,
                lines.len()
            )
        };
        frame.render_widget(Clear, area);
        frame.render_widget(
            Paragraph::new(lines)
                .scroll((u16::try_from(self.scroll).unwrap_or(u16::MAX), 0))
                .block(
                    Block::default()
                        .title(" Help ")
                        .title_bottom(Line::from(Span::styled(
                            footer,
                            Style::default().fg(theme().muted),
                        )))
                        .borders(Borders::ALL)
                        .border_type(ratatui::widgets::BorderType::Rounded)
                        .border_style(Style::default().fg(theme().blue)),
                )
                .style(Style::default().fg(theme().text).bg(theme().surface)),
            area,
        );
    }
}

fn help_lines() -> Vec<Line<'static>> {
    vec![
        help_line("PANELS", "- / Tab", "toggle explorer / diff"),
        help_line("", "/", "search files"),
        help_line("", "?", "open / close this help"),
        Line::default(),
        help_line("NAVIGATION", "j k / ↑ ↓", "move"),
        help_line("", "Ctrl+U / Ctrl+D", "half-page up / down"),
        help_line("", "gg / G", "start / end"),
        help_line("", "{line}gg", "jump to line"),
        help_line("", "h l / ← →", "close / open tree node"),
        help_line("", "{ / }", "previous / next revision"),
        help_line("", "click / wheel", "move cursor / scroll"),
        Line::default(),
        help_line("REVIEW", "c", "select review lines"),
        help_line("", "r", "suggest replacement for selection"),
        help_line("", "Space", "mark file reviewed / reopen"),
        help_line("", "v / Shift+V", "visual character / line mode"),
        help_line("", "y", "yank visual selection"),
        help_line("", "Shift+Y", "copy comments and suggestions"),
        help_line("", "Shift+R", "clear watched revisions"),
        help_line("", "Enter / double click", "add or edit comment"),
        help_line("", "[ / ]", "previous / next comment"),
        help_line("", "e", "open file in $EDITOR"),
        help_line("", "d", "delete comment"),
        help_line("", "u", "undo deleted comment"),
        Line::default(),
        help_line("EDITOR", "Enter", "save comment or suggestion"),
        help_line("", "Shift+Enter", "insert newline"),
        help_line("", "Ctrl+A / Ctrl+E", "start / end of line"),
        help_line("", "Ctrl+K / Ctrl+U", "kill to end / start of line"),
        help_line("", "Esc", "cancel"),
        Line::default(),
        help_line("VIEWER", "q", "quit"),
    ]
}

fn help_line(section: &'static str, key: &'static str, description: &'static str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{section:<12}"),
            Style::default().fg(if section.is_empty() {
                theme().muted
            } else {
                theme().comment
            }),
        ),
        Span::styled(format!(" {key:<20} "), theme().chip(theme().blue)),
        Span::raw("  "),
        Span::styled(description, Style::default().fg(theme().text)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyEvent;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn modal_help_consumes_input_until_it_closes() {
        let mut help = Help::default();
        help.open();
        let ignored_key = Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        assert_eq!(help.event(&ignored_key), EventState::Consumed);
        assert!(help.is_open());
        let close_key = Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(help.event(&close_key), EventState::Consumed);
        assert!(!help.is_open());
        assert_eq!(help.event(&ignored_key), EventState::Ignored);
    }

    #[test]
    fn short_terminals_scroll_the_help_instead_of_hiding_its_tail() {
        let mut help = Help::default();
        help.open();
        let mut terminal = Terminal::new(TestBackend::new(72, 12)).unwrap();
        terminal
            .draw(|frame| help.draw(frame, frame.area()))
            .unwrap();
        assert!(help.max_scroll > 0);

        help.event(&Event::Key(KeyEvent::new(
            KeyCode::Char('G'),
            KeyModifiers::NONE,
        )));
        terminal
            .draw(|frame| help.draw(frame, frame.area()))
            .unwrap();
        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains("quit"));
    }

    #[test]
    fn help_does_not_scroll_when_everything_already_fits() {
        let mut help = Help::default();
        help.open();
        let mut terminal = Terminal::new(TestBackend::new(72, 60)).unwrap();
        terminal
            .draw(|frame| help.draw(frame, frame.area()))
            .unwrap();
        assert_eq!(help.max_scroll, 0);
    }
}
