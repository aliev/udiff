use super::{
    BG, BLUE, COMMENT, Focus, GREEN, MUTED, RED, SURFACE, TEXT, diff_pane::DiffPane,
    file_tree::FileTree, render::crop_spans, session::Session,
};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use std::time::{Duration, Instant};
use unicode_width::UnicodeWidthStr;

#[derive(Default)]
pub struct Statusline {
    notice: Option<(String, Instant)>,
}

pub struct View<'a> {
    pub session: &'a Session,
    pub pane: &'a DiffPane,
    pub tree: &'a FileTree,
    pub batch: Option<(usize, usize, u64)>,
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
            batch,
        } = view;
        let width = area.width as usize;
        let (mut left, right) = if focus == Focus::Filter {
            let prompt = vec![
                Span::styled(" /", Style::default().fg(BLUE).add_modifier(Modifier::BOLD)),
                Span::styled(tree.filter().to_owned(), Style::default().fg(TEXT)),
                Span::styled(" ", Style::default().bg(TEXT)),
            ];
            let right = if tree.no_match() {
                Span::styled(" no matches ", Style::default().fg(RED))
            } else {
                Span::styled(
                    " Enter accept · Esc cancel · Ctrl+U clear ",
                    Style::default().fg(MUTED),
                )
            };
            (prompt, right)
        } else {
            let (mode, color) = match focus {
                Focus::Files => (" FILES ", COMMENT),
                Focus::Editor => (" COMMENT ", GREEN),
                _ if pane.visual_mode.is_some() => (" VISUAL ", COMMENT),
                _ if pane.range_anchor.is_some() => (" COMMENT SELECT ", BLUE),
                _ if pane.file_view => (" FILE VIEW ", BLUE),
                _ => (" NORMAL ", BLUE),
            };
            let current = pane.current(&session.files);
            let detail = if pane.vim_command.is_empty() {
                current.path.clone()
            } else {
                pane.vim_command.clone()
            };
            let right = if let Some((message, shown_at)) = &self.notice
                && shown_at.elapsed() < Duration::from_secs(4)
            {
                format!(" {message} · ? help ")
            } else {
                let progress = format!(
                    "{}/{} viewed · {} comments",
                    session.viewed_files.len(),
                    session.files.len(),
                    session.comments.len()
                );
                let state = match focus {
                    Focus::Files => format!(" {progress} "),
                    Focus::Editor => " Enter save · Shift+Enter newline · Esc cancel ".into(),
                    _ => {
                        let line = &pane.active_lines(&session.files)[pane.cursor];
                        let location = match (line.old, line.new) {
                            (_, Some(number)) => format!("new L{number}"),
                            (Some(number), None) => format!("old L{number}"),
                            _ => "hunk".into(),
                        };
                        let percent = (pane.cursor + 1) * 100 / current.lines.len().max(1);
                        format!(" {location} · {percent}% · {progress} ")
                    }
                };
                format!(" ? help ·{state}")
            };
            (
                {
                    let mut spans = vec![
                        Span::styled(
                            mode,
                            Style::default()
                                .fg(BG)
                                .bg(color)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(format!(" {detail}"), Style::default().fg(TEXT)),
                    ];
                    if let Some((position, total, number)) = *batch {
                        spans.push(Span::styled(
                            format!(" · batch {position}/{total} (#{number})"),
                            Style::default().fg(MUTED),
                        ));
                    }
                    spans
                },
                Span::styled(right, Style::default().fg(MUTED)),
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
            Paragraph::new(Line::from(left)).style(Style::default().fg(MUTED).bg(SURFACE)),
            area,
        );
    }
}
