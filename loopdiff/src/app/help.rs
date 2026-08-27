use super::{BLUE, MUTED, SURFACE, TEXT};
use crossterm::event::{Event, KeyCode};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

#[derive(Default)]
pub struct Help {
    open: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EventState {
    Consumed,
    Ignored,
}

impl Help {
    pub fn open(&mut self) {
        self.open = true;
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn event(&mut self, event: &Event) -> EventState {
        if !self.open {
            return EventState::Ignored;
        }
        if matches!(event, Event::Key(key) if matches!(key.code, KeyCode::Esc | KeyCode::Char('?')))
        {
            self.open = false;
        }
        EventState::Consumed
    }

    pub fn draw(&mut self, frame: &mut Frame, root: Rect) {
        if !self.open {
            return;
        }
        let lines = vec![
            help_line("PANELS", "-", "toggle explorer / diff"),
            help_line("", "/", "search files"),
            help_line("", "?", "open / close this help"),
            Line::default(),
            help_line("NAVIGATION", "j k / ↑ ↓", "move"),
            help_line("", "Ctrl+U / Ctrl+D", "half-page up / down"),
            help_line("", "gg / G", "start / end"),
            help_line("", "{line}gg", "jump to line"),
            help_line("", "h l / ← →", "scroll explorer horizontally"),
            help_line("", "{ / }", "previous / next diffwatch batch"),
            Line::default(),
            help_line("REVIEW", "c", "select lines for a comment"),
            help_line("", "Space", "mark file viewed / unviewed"),
            help_line("", "v / Shift+V", "visual character / line mode"),
            help_line("", "y", "yank visual selection"),
            help_line("", "Shift+Y", "copy comments"),
            help_line("", "Shift+R", "clear watched batches"),
            help_line("", "Enter", "add or edit comment"),
            help_line("", "[ / ]", "previous / next comment"),
            help_line("", "e", "open file in $EDITOR"),
            help_line("", "d", "delete comment"),
            help_line("", "u", "undo deleted comment"),
            Line::default(),
            help_line("EDITOR", "Enter", "save comment"),
            help_line("", "Shift+Enter", "insert newline"),
            help_line("", "Esc", "cancel"),
            Line::default(),
            help_line("VIEWER", "q", "quit"),
        ];
        let width = root.width.saturating_sub(4).min(72);
        let desired_height = u16::try_from(lines.len())
            .unwrap_or(u16::MAX)
            .saturating_add(2);
        let height = root.height.saturating_sub(2).min(desired_height);
        let area = Rect {
            x: root.x + root.width.saturating_sub(width) / 2,
            y: root.y + root.height.saturating_sub(height) / 2,
            width,
            height,
        };
        frame.render_widget(Clear, area);
        frame.render_widget(
            Paragraph::new(lines)
                .block(
                    Block::default()
                        .title(" Help · ? or Esc to close ")
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(BLUE)),
                )
                .style(Style::default().fg(TEXT).bg(SURFACE)),
            area,
        );
    }
}

fn help_line(section: &'static str, key: &'static str, description: &'static str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{section:<12}"), Style::default().fg(MUTED)),
        Span::styled(format!("{key:<20}"), Style::default().fg(BLUE)),
        Span::styled(description, Style::default().fg(TEXT)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyModifiers};

    #[test]
    fn modal_help_consumes_input_until_it_closes() {
        let mut help = Help::default();
        help.open();
        let ignored_key = Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        assert_eq!(help.event(&ignored_key), EventState::Consumed);
        assert!(help.is_open());
        let close_key = Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(help.event(&close_key), EventState::Consumed);
        assert!(!help.is_open());
        assert_eq!(help.event(&ignored_key), EventState::Ignored);
    }
}
