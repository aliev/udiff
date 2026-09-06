use super::{
    Focus,
    comment_editor::{CommentEditor, Mode as EditorMode},
    diff_pane::DiffPane,
    file_tree::FileTree,
    render::crop_spans,
    session::Session,
    view_helpers::anchor_position,
};
use crate::theme::theme;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use std::time::{Duration, Instant};
use unicode_width::UnicodeWidthStr;

/// Below this width the hints are trimmed so the mode badge still fits.
const COMPACT_WIDTH: usize = 72;

#[derive(Default)]
pub struct Statusline {
    notice: Option<(String, Instant)>,
}

pub struct View<'a> {
    pub session: &'a Session,
    pub pane: &'a DiffPane,
    pub tree: &'a FileTree,
    pub editor: &'a CommentEditor,
    pub revision: Option<(usize, usize, u64, bool)>,
}

impl Statusline {
    pub fn notice(&mut self, message: impl Into<String>) {
        self.notice = Some((message.into(), Instant::now()));
    }

    #[cfg(test)]
    pub fn clear_notice(&mut self) {
        self.notice = None;
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect, focus: Focus, view: &View<'_>) {
        let View {
            session,
            pane,
            tree,
            editor,
            revision,
        } = view;
        let width = area.width as usize;
        let (mut left, right) = if focus == Focus::Filter {
            let prompt = vec![
                Span::styled(
                    " /",
                    Style::default()
                        .fg(theme().blue)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(tree.filter().to_owned(), Style::default().fg(theme().text)),
                Span::styled(" ", theme().caret()),
            ];
            let right = if tree.no_match() {
                Span::styled(" no matches ", Style::default().fg(theme().red))
            } else {
                Span::styled(
                    " Enter accept · Esc cancel · Ctrl+U clear ",
                    Style::default().fg(theme().muted),
                )
            };
            (prompt, right)
        } else {
            let current = pane.current(&session.files);
            let active_comment = session.comments.iter().find(|comment| {
                comment.path == current.path
                    && anchor_position(current, comment) == Some(pane.cursor)
            });
            let (mode, color) = match focus {
                Focus::Files => (" FILES ", theme().comment),
                Focus::Editor if editor.mode == EditorMode::Suggestion => {
                    (" SUGGESTION ", theme().green)
                }
                Focus::Editor => (" COMMENT ", theme().green),
                _ if pane.visual_mode.is_some() => (" VISUAL ", theme().comment),
                _ if pane.range_anchor.is_some() => (" REVIEW SELECT ", theme().blue),
                _ if active_comment.is_some_and(|comment| comment.body.is_suggestion()) => {
                    (" SUGGESTION ", theme().green)
                }
                _ if active_comment.is_some() => (" COMMENT ", theme().comment),
                _ => (" NORMAL ", theme().blue),
            };
            let compact = width < COMPACT_WIDTH;
            let right = if let Some((message, shown_at)) = &self.notice
                && shown_at.elapsed() < Duration::from_secs(4)
            {
                if compact {
                    format!(" {message} ")
                } else {
                    format!(" {message} · ? help ")
                }
            } else {
                let review_complete = !session.files.is_empty()
                    && session.reviewed_files.len() == session.files.len();
                // Each state offers a full hint and a trimmed one for narrow
                // terminals, where the hint would otherwise crowd out the mode.
                let (full, short): (String, String) = match focus {
                    Focus::Files => (
                        " j/k navigate · Space reviewed ".into(),
                        " j/k · Space ".into(),
                    ),
                    Focus::Editor => (
                        " Enter save · Shift+Enter newline · Esc cancel ".into(),
                        " Enter save · Esc ".into(),
                    ),
                    _ if pane.range_anchor.is_some() => (
                        " j/k extend · Enter comment · r suggest · c cancel ".into(),
                        " Enter comment · r suggest ".into(),
                    ),
                    _ if pane.visual_mode.is_some() => (
                        " h/j/k/l select · y copy · Esc cancel ".into(),
                        " y copy · Esc ".into(),
                    ),
                    _ if active_comment.is_some() => (
                        " Enter edit · [ / ] browse comments · Space reviewed ".into(),
                        " Enter edit · [ / ] ".into(),
                    ),
                    _ if review_complete => (
                        " ✓ Review complete · Shift+Y copy comments ".into(),
                        " ✓ complete · Shift+Y ".into(),
                    ),
                    _ if session.reviewed_files.contains(&pane.file) => (
                        " Space reopen review · [/] comments · ? help ".into(),
                        " Space reopen · ? help ".into(),
                    ),
                    _ => {
                        let line = &pane.active_lines(&session.files)[pane.cursor];
                        let location = match (line.old, line.new) {
                            (_, Some(number)) => format!("new L{number}"),
                            (Some(number), None) => format!("old L{number}"),
                            _ => "hunk".into(),
                        };
                        let percent = (pane.cursor + 1) * 100 / current.lines.len().max(1);
                        (
                            format!(
                                " c comment · r suggest · Space reviewed · {location} · {percent}% "
                            ),
                            format!(" {location} · {percent}% "),
                        )
                    }
                };
                if compact { short } else { full }
            };
            (
                {
                    let mut spans = vec![Span::styled(
                        mode,
                        Style::default().fg(color).add_modifier(Modifier::BOLD),
                    )];
                    if !pane.vim_command.is_empty() {
                        spans.push(Span::styled(
                            format!(" {}", pane.vim_command),
                            Style::default().fg(theme().text),
                        ));
                    }
                    if let Some((position, total, number, is_context)) = revision.as_ref() {
                        let identity = if *is_context {
                            "context".to_owned()
                        } else {
                            format!("#{number}")
                        };
                        spans.push(Span::styled(
                            format!(" · revision {position}/{total} · {identity}"),
                            Style::default().fg(theme().muted),
                        ));
                    }
                    spans
                },
                Span::styled(right, Style::default().fg(theme().muted)),
            )
        };
        let right_width = UnicodeWidthStr::width(right.content.as_ref()).min(width);
        let left_width = width.saturating_sub(right_width);
        left = crop_spans(left, 0, left_width);
        let rendered = left
            .iter()
            .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
            .sum::<usize>();
        left.push(Span::raw(" ".repeat(left_width.saturating_sub(rendered))));
        left.push(right);
        frame.render_widget(
            Paragraph::new(Line::from(left))
                .style(Style::default().fg(theme().muted).bg(theme().surface)),
            area,
        );
    }
}
