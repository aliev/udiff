use super::{
    BG, BLUE, BORDER, COMMENT, Focus, GREEN, GREEN_BG, HUNK_BG, MUTED, RED, RED_BG, SELECT_BG,
    SURFACE, TEXT,
    comment_editor::CommentEditor,
    diff_pane::{DiffPane, VisualMode},
    render::inline_message_lines,
    session::Session,
    view_helpers::{
        anchor_position, apply_block_cursor, apply_character_selection, editor_visual_rows,
        expand_tabs, expanded_character_column, line_in_note, ordered, ordered_position,
        wrap_code_line, wrapped_scroll,
    },
};
use crate::model::{DiffLine, FileDiff, FileStatus, FileViewChange, LineKind, file_view_changes};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use regex::Regex;
use unicode_width::UnicodeWidthStr;

#[cfg(test)]
impl DiffPane {
    pub(super) fn line_for_test<'a>(
        &mut self,
        session: &Session,
        editor: &CommentEditor,
        focus: Focus,
        line: &'a DiffLine,
        position: usize,
        width: usize,
    ) -> Line<'a> {
        Renderer {
            pane: self,
            session,
            editor,
            focus,
        }
        .diff_line(line, position, width)
    }

    pub(super) fn editor_lines_for_test<'a>(
        &mut self,
        session: &Session,
        editor: &CommentEditor,
        focus: Focus,
        output: (&mut Vec<Line<'a>>, &mut Vec<Option<usize>>),
        layout: (&str, usize),
    ) {
        let (lines, map) = output;
        let (title, width) = layout;
        Renderer {
            pane: self,
            session,
            editor,
            focus,
        }
        .append_editor(lines, map, title, width);
    }
}

pub(super) fn render(
    pane: &mut DiffPane,
    frame: &mut Frame,
    area: Rect,
    session: &Session,
    editor: &CommentEditor,
    focus: Focus,
) {
    Renderer {
        pane,
        session,
        editor,
        focus,
    }
    .draw_main(frame, area);
}

struct Renderer<'a> {
    pane: &'a mut DiffPane,
    session: &'a Session,
    editor: &'a CommentEditor,
    focus: Focus,
}

impl Renderer<'_> {
    fn current(&self) -> &FileDiff {
        self.pane.current(&self.session.files)
    }

    fn active_lines(&self) -> &[DiffLine] {
        self.pane.active_lines(&self.session.files)
    }

    fn ensure_visible(&mut self, height: usize) {
        if self.pane.cursor < self.pane.scroll {
            self.pane.scroll = self.pane.cursor;
        }
        if self.pane.cursor >= self.pane.scroll + height.saturating_sub(1) {
            self.pane.scroll = self.pane.cursor.saturating_sub(height.saturating_sub(2));
        }
    }

    fn visual_line_selected(&self, row: usize) -> bool {
        match self.pane.visual_mode {
            Some(VisualMode::Line { anchor_row }) => {
                let (start, end) = ordered(anchor_row, self.pane.cursor);
                start <= row && row <= end
            }
            _ => false,
        }
    }

    fn visual_character_range(&self, row: usize) -> Option<(usize, usize)> {
        let VisualMode::Character {
            anchor_row,
            anchor_col,
        } = self.pane.visual_mode?
        else {
            return None;
        };
        let ((start_row, start_col), (end_row, end_col)) = ordered_position(
            (anchor_row, anchor_col),
            (self.pane.cursor, self.pane.visual_col),
        );
        if !(start_row..=end_row).contains(&row) {
            return None;
        }
        let length = self.active_lines()[row].text.chars().count();
        if length == 0 {
            return None;
        }
        let start = if row == start_row { start_col } else { 0 }.min(length - 1);
        let end = if row == end_row {
            end_col.min(length - 1)
        } else {
            length - 1
        };
        Some((start, end))
    }

    fn in_range(&self, position: usize) -> bool {
        if self.pane.range_anchor.is_none() {
            return false;
        }
        let (start, end) = self.pane.selected_bounds();
        start <= position && position <= end
    }

    fn annotated(&self, position: usize) -> bool {
        let file = self.current();
        self.session.comments.iter().any(|comment| {
            comment.path == file.path
                && (anchor_position(file, comment) == Some(position)
                    || line_in_note(&file.lines[position], comment))
        })
    }

    fn draw_main(&mut self, f: &mut Frame, a: Rect) {
        let parts = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(2), Constraint::Min(3)])
            .split(a);
        let file = self.current();
        let shown_path = if file.status == FileStatus::Renamed {
            format!(
                "{} → {}",
                file.old_path.as_deref().unwrap_or("?"),
                file.path
            )
        } else {
            file.path.clone()
        };
        let file_header = vec![
            Span::styled(
                format!(" {shown_path}"),
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(format!("+{}", file.additions()), Style::default().fg(GREEN)),
            Span::styled(format!(" −{}", file.deletions()), Style::default().fg(RED)),
            Span::styled(
                if self.pane.file_view {
                    "  FILE"
                } else {
                    "  DIFF"
                },
                Style::default().fg(BLUE).add_modifier(Modifier::BOLD),
            ),
        ];
        f.render_widget(
            Paragraph::new(Line::from(file_header))
                .block(
                    Block::default()
                        .borders(Borders::BOTTOM)
                        .border_style(Style::default().fg(
                            if matches!(self.focus, Focus::Diff | Focus::Editor) {
                                BLUE
                            } else {
                                BORDER
                            },
                        )),
                )
                .style(Style::default().bg(SURFACE)),
            parts[0],
        );
        self.pane.area = parts[1];
        if self.pane.file_view {
            self.draw_file(f, parts[1]);
        } else {
            self.draw_diff(f, parts[1]);
        }
    }

    fn draw_file(&mut self, f: &mut Frame, area: Rect) {
        let height = area.height as usize;
        self.ensure_visible(height);
        let width = area.width as usize;
        let lines = self.active_lines().to_vec();
        self.pane.scroll = wrapped_scroll(
            self.pane.scroll,
            self.pane.cursor,
            height,
            width,
            9,
            &lines,
            false,
        );
        let changes = file_view_changes(self.current());
        let visible = (self.pane.scroll..lines.len())
            .flat_map(|position| {
                let change = if self.current().status == FileStatus::Deleted {
                    Some(FileViewChange::Removed)
                } else {
                    changes.iter().find_map(|(line, change)| {
                        (*line as usize == position + 1).then_some(*change)
                    })
                };
                wrap_code_line(
                    self.file_line(&lines[position], position, change, width),
                    2,
                    width,
                )
                .into_iter()
                .map(move |line| (line, position))
            })
            .take(height)
            .collect::<Vec<_>>();
        self.pane.row_map = visible
            .iter()
            .map(|(_, position)| Some(*position))
            .collect();
        f.render_widget(
            Paragraph::new(
                visible
                    .into_iter()
                    .map(|(line, _)| line)
                    .collect::<Vec<_>>(),
            )
            .style(Style::default().bg(BG)),
            area,
        );
    }

    fn file_line<'a>(
        &self,
        line: &'a DiffLine,
        position: usize,
        change: Option<FileViewChange>,
        width: usize,
    ) -> Line<'a> {
        let background = if self.visual_line_selected(position) {
            SELECT_BG
        } else {
            BG
        };
        let (marker, marker_color) = match change {
            Some(FileViewChange::Added) => ("▌", GREEN),
            Some(FileViewChange::Modified) => ("▌", BLUE),
            Some(FileViewChange::Deleted) => ("▾", RED),
            Some(FileViewChange::Removed) => ("▌", RED),
            None => (" ", MUTED),
        };
        let mut spans = vec![
            Span::styled(marker, Style::default().fg(marker_color).bg(background)),
            Span::styled(
                format!("{:>6}  ", position + 1),
                Style::default().fg(MUTED).bg(background),
            ),
        ];
        if line.syntax.is_empty() {
            spans.push(Span::styled(line.text.clone(), Style::default().fg(TEXT)));
        } else {
            for syntax in &line.syntax {
                let mut style =
                    Style::default().fg(Color::Rgb(syntax.rgb.0, syntax.rgb.1, syntax.rgb.2));
                if syntax.bold {
                    style = style.add_modifier(Modifier::BOLD);
                }
                if syntax.italic {
                    style = style.add_modifier(Modifier::ITALIC);
                }
                spans.push(Span::styled(syntax.text.clone(), style));
            }
        }
        for span in &mut spans {
            if span.style.bg.is_none() {
                span.style = span.style.bg(background);
            }
        }
        expand_tabs(&mut spans, 2);
        if let Some((raw_start, raw_end)) = self.visual_character_range(position) {
            let cursor = (position == self.pane.cursor).then(|| {
                expanded_character_column(
                    &line.text,
                    self.pane.visual_col.clamp(raw_start, raw_end),
                )
            });
            let start = expanded_character_column(&line.text, raw_start);
            let end = expanded_character_column(&line.text, raw_end + 1).saturating_sub(1);
            apply_character_selection(&mut spans, 2, start, end, cursor);
        } else if position == self.pane.cursor
            && self.pane.visual_mode.is_none()
            && self.focus == Focus::Diff
        {
            apply_block_cursor(
                &mut spans,
                2,
                expanded_character_column(&line.text, self.pane.visual_col),
                background,
            );
        }
        let content_width = spans
            .iter()
            .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
            .sum::<usize>();
        if content_width < width {
            spans.push(Span::styled(
                " ".repeat(width - content_width),
                Style::default().bg(background),
            ));
        }
        Line::from(spans)
    }

    fn draw_diff(&mut self, f: &mut Frame, a: Rect) {
        let height = a.height as usize;
        self.ensure_visible(height);
        let mut lines = Vec::new();
        let mut map = Vec::new();
        let file = self.current().clone();
        self.pane.scroll = wrapped_scroll(
            self.pane.scroll,
            self.pane.cursor,
            height,
            a.width as usize,
            13,
            &file.lines,
            true,
        );
        let viewport_starts_with_hunk = file
            .lines
            .get(self.pane.scroll)
            .is_some_and(|line| line.kind == LineKind::Hunk);
        if !viewport_starts_with_hunk {
            if let Some(sticky) = file.lines[..self.pane.scroll.min(file.lines.len())]
                .iter()
                .rposition(|line| line.kind == LineKind::Hunk)
            {
                for line in wrap_code_line(
                    self.diff_line(&file.lines[sticky], sticky, a.width as usize),
                    3,
                    a.width as usize,
                ) {
                    lines.push(line);
                    map.push(Some(sticky));
                }
            }
        }
        for p in self.pane.scroll..file.lines.len() {
            if lines.len() >= height {
                break;
            }
            let l = &file.lines[p];
            for line in wrap_code_line(self.diff_line(l, p, a.width as usize), 3, a.width as usize)
            {
                lines.push(line);
                map.push(Some(p));
            }
            let editor_here = Some(p) == self.editor.anchor && self.focus == Focus::Editor;
            if editor_here {
                let title = if self.editor.editing_key.is_some() {
                    "Edit comment"
                } else {
                    "Add comment"
                };
                self.append_editor(&mut lines, &mut map, title, a.width as usize);
            }
            for n in self
                .session
                .comments
                .iter()
                .filter(|n| n.path == file.path && anchor_position(&file, n) == Some(p))
            {
                for comment_line in inline_message_lines(&n.text, a.width as usize) {
                    lines.push(comment_line);
                    map.push(None);
                }
            }
        }
        self.pane.row_map = map;
        f.render_widget(Paragraph::new(lines).style(Style::default().bg(BG)), a);
    }

    fn append_editor<'a>(
        &self,
        lines: &mut Vec<Line<'a>>,
        map: &mut Vec<Option<usize>>,
        title: &str,
        width: usize,
    ) {
        lines.push(Line::from(Span::styled(
            format!("             ┌─ {title}"),
            Style::default().fg(BLUE),
        )));
        map.push(None);
        const EDITOR_PREFIX_WIDTH: usize = 15;
        let visual_rows = editor_visual_rows(
            &self.editor.text,
            width.saturating_sub(EDITOR_PREFIX_WIDTH).max(1),
        );
        let cursor_row = visual_rows
            .iter()
            .rposition(|(start, _)| *start <= self.editor.cursor)
            .unwrap_or(0);
        for (index, (start, end)) in visual_rows.into_iter().enumerate() {
            let edit_line = &self.editor.text[start..end];
            let mut editor_spans = vec![Span::raw("             │ ")];
            if index == cursor_row {
                let cursor_column = self
                    .editor
                    .cursor
                    .saturating_sub(start)
                    .min(edit_line.len());
                let before = &edit_line[..cursor_column];
                let after = &edit_line[cursor_column..];
                editor_spans.push(Span::styled(before.to_owned(), Style::default().fg(TEXT)));
                if let Some(character) = after.chars().next() {
                    editor_spans.push(Span::styled(
                        character.to_string(),
                        Style::default().fg(BG).bg(TEXT),
                    ));
                    editor_spans.push(Span::styled(
                        after[character.len_utf8()..].to_owned(),
                        Style::default().fg(TEXT),
                    ));
                } else {
                    editor_spans.push(Span::styled(" ", Style::default().bg(TEXT)));
                }
            } else {
                editor_spans.push(Span::styled(
                    edit_line.to_owned(),
                    Style::default().fg(TEXT),
                ));
            }
            lines.push(Line::from(editor_spans));
            map.push(None);
        }
        lines.push(Line::from(Span::styled(
            "             └─ Enter save · Shift+Enter newline · Esc cancel",
            Style::default().fg(MUTED),
        )));
        map.push(None);
    }

    fn diff_line<'a>(&self, l: &'a DiffLine, p: usize, width: usize) -> Line<'a> {
        let base_bg = match l.kind {
            LineKind::Add => GREEN_BG,
            LineKind::Remove => RED_BG,
            LineKind::Hunk => HUNK_BG,
            _ => BG,
        };
        let bg = if self.visual_line_selected(p) {
            SELECT_BG
        } else {
            base_bg
        };
        let sign = if self.in_range(p) {
            Span::styled("▌", Style::default().fg(BLUE))
        } else if self.annotated(p) {
            Span::styled("▌", Style::default().fg(COMMENT))
        } else {
            Span::raw(" ")
        };
        let old = l.old.map_or("    ".into(), |v| format!("{v:>4}"));
        let new = l.new.map_or("    ".into(), |v| format!("{v:>4}"));
        let marker = match l.kind {
            LineKind::Add => Style::default().fg(GREEN),
            LineKind::Remove => Style::default().fg(RED),
            _ => Style::default().fg(MUTED),
        };
        let mut spans = vec![
            sign,
            Span::styled(format!("{old} {new} "), Style::default().fg(MUTED)),
            Span::styled(format!("{} ", l.marker()), marker),
        ];
        if l.kind == LineKind::Hunk {
            let re = Regex::new(r"^(@@.*?@@)(.*)$").unwrap();
            if let Some(c) = re.captures(&l.text) {
                spans.push(Span::styled(c[1].to_string(), Style::default().fg(BLUE)));
                spans.push(Span::styled(c[2].to_string(), Style::default().fg(MUTED)));
            } else {
                spans.push(Span::styled(l.text.clone(), Style::default().fg(BLUE)));
            }
        } else if l.syntax.is_empty() {
            spans.push(Span::styled(l.text.clone(), Style::default().fg(TEXT)));
        } else {
            for s in &l.syntax {
                let mut st = Style::default().fg(Color::Rgb(s.rgb.0, s.rgb.1, s.rgb.2));
                if s.bold {
                    st = st.add_modifier(Modifier::BOLD)
                }
                if s.italic {
                    st = st.add_modifier(Modifier::ITALIC)
                }
                spans.push(Span::styled(s.text.clone(), st));
            }
        }
        for span in &mut spans {
            if span.style.bg.is_none() {
                span.style = span.style.bg(bg);
            }
        }
        expand_tabs(&mut spans, 3);
        if let Some((raw_start, raw_end)) = self.visual_character_range(p) {
            let cursor = (p == self.pane.cursor).then(|| {
                expanded_character_column(&l.text, self.pane.visual_col.clamp(raw_start, raw_end))
            });
            let start = expanded_character_column(&l.text, raw_start);
            let end = expanded_character_column(&l.text, raw_end + 1).saturating_sub(1);
            apply_character_selection(&mut spans, 3, start, end, cursor);
        } else if p == self.pane.cursor
            && self.pane.visual_mode.is_none()
            && self.focus == Focus::Diff
        {
            apply_block_cursor(
                &mut spans,
                3,
                expanded_character_column(&l.text, self.pane.visual_col),
                bg,
            );
        }
        let content_width = spans
            .iter()
            .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
            .sum::<usize>();
        if content_width < width {
            spans.push(Span::styled(
                " ".repeat(width - content_width),
                Style::default().bg(bg),
            ));
        }
        Line::from(spans)
    }
}
