use super::{
    BG, BLUE, BORDER, COMMENT, COMMENT_BG, Focus, GREEN, GREEN_BG, HUNK_BG, MUTED, RED, RED_BG,
    SELECT_BG, SURFACE, TEXT,
    comment_editor::{CommentEditor, Mode as EditorMode},
    diff_pane::{DiffPane, VisualMode},
    render::{inline_comment_lines, styled_syntax_spans},
    session::Session,
    view_helpers::{
        anchor_position, apply_block_cursor, apply_character_selection, editor_visual_rows,
        expand_tabs, expanded_character_column, line_in_comment, ordered, ordered_position,
        wrap_code_line, wrapped_scroll,
    },
};
use crate::model::{
    DiffLine, FileDiff, FileStatus, FileViewChange, LineKind, file_view_changes, highlight_source,
};
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
                    || line_in_comment(&file.lines[position], comment))
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
        let mut left_header = vec![Span::raw(" ")];
        left_header.extend(crate::app::render::file_status_spans(file.status));
        left_header.extend([
            Span::styled(
                format!("  {shown_path}"),
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("   +{}", file.additions()),
                Style::default().fg(GREEN),
            ),
            Span::styled(format!(" −{}", file.deletions()), Style::default().fg(RED)),
        ]);
        let right_header = vec![
            Span::styled(
                if self.pane.file_view {
                    " FILE "
                } else {
                    " DIFF "
                },
                Style::default()
                    .fg(BLUE)
                    .bg(SELECT_BG)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
        ];
        f.render_widget(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(
                    if matches!(self.focus, Focus::Diff | Focus::Editor) {
                        BLUE
                    } else {
                        BORDER
                    },
                ))
                .style(Style::default().bg(SURFACE)),
            parts[0],
        );
        let right_width = right_header
            .iter()
            .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
            .sum::<usize>()
            .min(parts[0].width as usize) as u16;
        let header_columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(0), Constraint::Length(right_width)])
            .split(Rect {
                height: 1,
                ..parts[0]
            });
        f.render_widget(
            Paragraph::new(Line::from(left_header)).style(Style::default().bg(SURFACE)),
            header_columns[0],
        );
        f.render_widget(
            Paragraph::new(Line::from(right_header))
                .alignment(ratatui::layout::Alignment::Right)
                .style(Style::default().bg(SURFACE)),
            header_columns[1],
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
        let code_background = if self.visual_line_selected(position) {
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
            Span::styled(marker, Style::default().fg(marker_color).bg(BG)),
            Span::styled(
                format!("{:>6}  ", position + 1),
                Style::default().fg(MUTED).bg(BG),
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
        for span in &mut spans[2..] {
            if span.style.bg.is_none() {
                span.style = span.style.bg(code_background);
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
                code_background,
            );
        }
        let content_width = spans
            .iter()
            .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
            .sum::<usize>();
        if content_width < width {
            spans.push(Span::styled(
                " ".repeat(width - content_width),
                Style::default().bg(code_background),
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
        self.pane.scroll = review_aware_scroll(
            self.pane.scroll,
            self.pane.cursor,
            height,
            a.width as usize,
            &file,
            self.session,
        );
        let viewport_starts_with_hunk = file
            .lines
            .get(self.pane.scroll)
            .is_some_and(|line| line.kind == LineKind::Hunk);
        if !viewport_starts_with_hunk
            && let Some(sticky) = file.lines[..self.pane.scroll.min(file.lines.len())]
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
                let kind = if self.editor.mode == EditorMode::Suggestion {
                    "SUGGESTION"
                } else {
                    "COMMENT"
                };
                let title = if let Some(key) = &self.editor.editing_key {
                    let location = self
                        .session
                        .comments
                        .iter()
                        .find(|comment| &comment.id == key)
                        .map(|comment| comment.short_location())
                        .unwrap_or_else(|| "selection".into());
                    format!("EDIT {kind} · {location}")
                } else {
                    let (start, end) = self.pane.selected_bounds();
                    let lines = end - start + 1;
                    format!(
                        "NEW {kind} · {lines} line{} selected",
                        if lines == 1 { "" } else { "s" }
                    )
                };
                self.append_editor(&mut lines, &mut map, &title, a.width as usize);
            }
            for (number, n) in self
                .session
                .comments
                .iter()
                .filter(|n| n.path == file.path)
                .enumerate()
                .filter(|(_, n)| anchor_position(&file, n) == Some(p))
            {
                for comment_line in inline_comment_lines(n, number + 1, a.width as usize) {
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
        const EDITOR_PREFIX_WIDTH: usize = 13;
        lines.push(editor_card_line(
            vec![Span::styled(
                format!("╭─ {title}"),
                Style::default().fg(COMMENT).bg(COMMENT_BG),
            )],
            width,
            EDITOR_PREFIX_WIDTH,
        ));
        map.push(None);
        let visual_rows = editor_visual_rows(
            &self.editor.text,
            width.saturating_sub(EDITOR_PREFIX_WIDTH + 2).max(1),
        );
        let syntax = (self.editor.mode == EditorMode::Suggestion)
            .then(|| highlight_source(&self.current().path, &self.editor.text));
        let cursor_row = visual_rows
            .iter()
            .rposition(|(start, _)| *start <= self.editor.cursor)
            .unwrap_or(0);
        for (index, (start, end)) in visual_rows.into_iter().enumerate() {
            let edit_line = &self.editor.text[start..end];
            let mut editor_spans = vec![Span::styled(
                "┃ ",
                Style::default().fg(COMMENT).bg(COMMENT_BG),
            )];
            let mut code_spans = syntax
                .as_ref()
                .and_then(|lines| {
                    let logical_start = self.editor.text[..start]
                        .rfind('\n')
                        .map_or(0, |position| position + 1);
                    let logical_line = self.editor.text[..logical_start]
                        .bytes()
                        .filter(|byte| *byte == b'\n')
                        .count();
                    lines.get(logical_line).map(|spans| {
                        styled_syntax_spans(
                            spans,
                            start - logical_start,
                            end - logical_start,
                            COMMENT_BG,
                        )
                    })
                })
                .unwrap_or_default();
            if code_spans.is_empty() && !edit_line.is_empty() {
                code_spans.push(Span::styled(
                    edit_line.to_owned(),
                    Style::default().fg(TEXT).bg(COMMENT_BG),
                ));
            }
            if index == cursor_row {
                let cursor_column = self
                    .editor
                    .cursor
                    .saturating_sub(start)
                    .min(edit_line.len());
                let cursor_column = edit_line[..cursor_column].chars().count();
                apply_block_cursor(&mut code_spans, 0, cursor_column, COMMENT_BG);
            }
            editor_spans.extend(code_spans);
            lines.push(editor_card_line(editor_spans, width, EDITOR_PREFIX_WIDTH));
            map.push(None);
        }
        lines.push(editor_card_line(
            vec![Span::styled(
                "╰─ Enter save · Shift+Enter newline · Esc cancel",
                Style::default().fg(MUTED).bg(COMMENT_BG),
            )],
            width,
            EDITOR_PREFIX_WIDTH,
        ));
        map.push(None);
    }

    fn diff_line<'a>(&self, l: &'a DiffLine, p: usize, width: usize) -> Line<'a> {
        let base_bg = match l.kind {
            LineKind::Add => GREEN_BG,
            LineKind::Remove => RED_BG,
            LineKind::Hunk => HUNK_BG,
            _ => BG,
        };
        let code_bg = if self.visual_line_selected(p) {
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
        for (index, span) in spans.iter_mut().enumerate() {
            if span.style.bg.is_none() {
                span.style = span.style.bg(if index < 3 { base_bg } else { code_bg });
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
                code_bg,
            );
        }
        let content_width = spans
            .iter()
            .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
            .sum::<usize>();
        if content_width < width {
            spans.push(Span::styled(
                " ".repeat(width - content_width),
                Style::default().bg(code_bg),
            ));
        }
        Line::from(spans)
    }
}

fn review_aware_scroll(
    mut scroll: usize,
    cursor: usize,
    height: usize,
    width: usize,
    file: &FileDiff,
    session: &Session,
) -> usize {
    let review_rows = session
        .comments
        .iter()
        .filter(|comment| comment.path == file.path)
        .enumerate()
        .filter_map(|(number, comment)| {
            anchor_position(file, comment).map(|anchor| {
                (
                    anchor,
                    inline_comment_lines(comment, number + 1, width).len(),
                )
            })
        })
        .collect::<Vec<_>>();
    while scroll < cursor {
        let code_rows = file.lines[scroll..=cursor]
            .iter()
            .map(|line| super::view_helpers::wrapped_code_row_count(&line.text, 13, width))
            .sum::<usize>();
        let inline_rows = review_rows
            .iter()
            .filter(|(anchor, _)| scroll <= *anchor && *anchor < cursor)
            .map(|(_, rows)| rows)
            .sum::<usize>();
        let sticky_rows = if file.lines[scroll].kind != LineKind::Hunk {
            file.lines[..scroll]
                .iter()
                .rposition(|line| line.kind == LineKind::Hunk)
                .map(|position| {
                    super::view_helpers::wrapped_code_row_count(
                        &file.lines[position].text,
                        3,
                        width,
                    )
                })
                .unwrap_or(0)
        } else {
            0
        };
        if code_rows
            .saturating_add(inline_rows)
            .saturating_add(sticky_rows)
            <= height
        {
            break;
        }
        scroll += 1;
    }
    scroll
}

fn editor_card_line<'a>(mut card: Vec<Span<'a>>, width: usize, prefix_width: usize) -> Line<'a> {
    let card_width = width.saturating_sub(prefix_width);
    let rendered = card
        .iter()
        .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
        .sum::<usize>();
    if rendered < card_width {
        card.push(Span::styled(
            " ".repeat(card_width - rendered),
            Style::default().bg(COMMENT_BG),
        ));
    }
    let mut spans = vec![Span::styled(
        " ".repeat(prefix_width.min(width)),
        Style::default().bg(BG),
    )];
    spans.extend(card);
    Line::from(spans)
}
