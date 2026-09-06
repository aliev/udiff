use super::{
    Focus,
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
use crate::{
    highlight::highlight_source,
    model::{DiffLine, FileDiff, FileStatus, LineKind},
    theme::theme,
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
};
use regex::Regex;
use unicode_width::UnicodeWidthStr;

const DIFF_PREFIX_WIDTH: usize = 13;
/// Gutter column of the change marker, reused for the soft-wrap marker so a
/// continuation row lines up with the `+`/`-` above it.
const WRAP_MARKER_COLUMN: usize = 11;
const EDITOR_PREFIX_WIDTH: usize = 13;
const EDITOR_TEXT_INSET: usize = 2;
const SCROLL_MARGIN_ROWS: usize = 3;

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
        self.pane.area = Rect::default();
        if a.width == 0 || a.height == 0 {
            self.pane.row_map.clear();
            return;
        }
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
                Style::default()
                    .fg(theme().text)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("   +{}", file.additions()),
                Style::default().fg(theme().green),
            ),
            Span::styled(
                format!(" −{}", file.deletions()),
                Style::default().fg(theme().red),
            ),
        ]);
        let reviewed = self.session.reviewed_files.contains(&self.pane.file);
        let right_header = vec![
            Span::styled(
                format!(
                    " {}{}/{} ",
                    if reviewed { "\u{2713} " } else { "" },
                    self.pane.file + 1,
                    self.session.files.len()
                ),
                Style::default()
                    .fg(if reviewed {
                        theme().green
                    } else {
                        theme().blue
                    })
                    .bg(theme().select_bg)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
        ];
        f.render_widget(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(
                    if matches!(self.focus, Focus::Diff | Focus::Editor) {
                        theme().blue
                    } else {
                        theme().border
                    },
                ))
                .style(Style::default().bg(theme().surface)),
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
            Paragraph::new(Line::from(left_header)).style(Style::default().bg(theme().surface)),
            header_columns[0],
        );
        f.render_widget(
            Paragraph::new(Line::from(right_header))
                .alignment(ratatui::layout::Alignment::Right)
                .style(Style::default().bg(theme().surface)),
            header_columns[1],
        );
        // A one-column gutter for the scrollbar, only while the file overflows.
        let overflows = self.current().lines.len() > parts[1].height as usize;
        let body = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Min(0),
                Constraint::Length(u16::from(overflows).min(parts[1].width)),
            ])
            .split(parts[1]);
        self.pane.area = body[0];
        self.draw_diff(f, body[0]);
        if overflows {
            self.draw_scrollbar(f, body[1]);
        }
    }

    fn draw_scrollbar(&self, f: &mut Frame, a: Rect) {
        let lines = self.current().lines.len();
        let mut state = ScrollbarState::new(lines)
            .position(self.pane.scroll)
            .viewport_content_length(a.height as usize);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None)
                .track_symbol(Some("\u{2502}"))
                .thumb_symbol("\u{2588}")
                .track_style(Style::default().fg(theme().border).bg(theme().bg))
                .thumb_style(Style::default().fg(theme().muted).bg(theme().bg)),
            a,
            &mut state,
        );
    }

    fn draw_diff(&mut self, f: &mut Frame, a: Rect) {
        let height = a.height as usize;
        self.ensure_visible(height);
        let mut lines = Vec::new();
        let mut map = Vec::new();
        let file = self.current().clone();
        let editor_active = self.focus == Focus::Editor && self.editor.anchor.is_some();
        let sticky_hunk = !editor_active;
        if editor_active {
            self.pane.scroll =
                editor_aware_scroll(height, a.width as usize, &file, self.session, self.editor);
        } else {
            self.pane.scroll = wrapped_scroll(
                self.pane.scroll,
                self.pane.cursor,
                height,
                a.width as usize,
                DIFF_PREFIX_WIDTH,
                &file.lines,
                sticky_hunk,
            );
            self.pane.scroll = review_aware_scroll(
                self.pane.scroll,
                self.pane.cursor,
                height,
                a.width as usize,
                &file,
                self.session,
                SCROLL_MARGIN_ROWS,
            );
        }
        let viewport_starts_with_hunk = file
            .lines
            .get(self.pane.scroll)
            .is_some_and(|line| line.kind == LineKind::Hunk);
        if sticky_hunk
            && !viewport_starts_with_hunk
            && let Some(sticky) = file.lines[..self.pane.scroll.min(file.lines.len())]
                .iter()
                .rposition(|line| line.kind == LineKind::Hunk)
        {
            for line in wrap_code_line(
                self.diff_line(&file.lines[sticky], sticky, a.width as usize),
                3,
                a.width as usize,
                Some(WRAP_MARKER_COLUMN),
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
            for line in wrap_code_line(
                self.diff_line(l, p, a.width as usize),
                3,
                a.width as usize,
                Some(WRAP_MARKER_COLUMN),
            ) {
                lines.push(line);
                map.push(Some(p));
            }
            let editor_here = Some(p) == self.editor.anchor && self.focus == Focus::Editor;
            if editor_here {
                let kind = if self.editor.mode == EditorMode::Suggestion {
                    "SUGGESTION"
                } else {
                    "theme().comment"
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
        f.render_widget(
            Paragraph::new(lines).style(Style::default().bg(theme().bg)),
            a,
        );
    }

    fn append_editor<'a>(
        &self,
        lines: &mut Vec<Line<'a>>,
        map: &mut Vec<Option<usize>>,
        title: &str,
        width: usize,
    ) {
        lines.push(editor_card_line(
            vec![Span::styled(
                format!("╭─ {title}"),
                Style::default().fg(theme().comment).bg(theme().comment_bg),
            )],
            width,
            EDITOR_PREFIX_WIDTH,
        ));
        map.push(None);
        let visual_rows = editor_visual_rows(&self.editor.text, editor_text_width(width));
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
                Style::default().fg(theme().comment).bg(theme().comment_bg),
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
                            theme().comment_bg,
                        )
                    })
                })
                .unwrap_or_default();
            if code_spans.is_empty() && !edit_line.is_empty() {
                code_spans.push(Span::styled(
                    edit_line.to_owned(),
                    Style::default().fg(theme().text).bg(theme().comment_bg),
                ));
            }
            if index == cursor_row {
                let cursor_column = self
                    .editor
                    .cursor
                    .saturating_sub(start)
                    .min(edit_line.len());
                let cursor_column = edit_line[..cursor_column].chars().count();
                apply_block_cursor(&mut code_spans, 0, cursor_column, theme().comment_bg);
            }
            editor_spans.extend(code_spans);
            lines.push(editor_card_line(editor_spans, width, EDITOR_PREFIX_WIDTH));
            map.push(None);
        }
        lines.push(editor_card_line(
            vec![Span::styled(
                "╰─ Enter save · Shift+Enter newline · Esc cancel",
                Style::default().fg(theme().muted).bg(theme().comment_bg),
            )],
            width,
            EDITOR_PREFIX_WIDTH,
        ));
        map.push(None);
    }

    fn diff_line<'a>(&self, l: &'a DiffLine, p: usize, width: usize) -> Line<'a> {
        let base_bg = match l.kind {
            LineKind::Add => theme().green_bg,
            LineKind::Remove => theme().red_bg,
            LineKind::Hunk => theme().hunk_bg,
            _ => theme().bg,
        };
        let code_bg = if self.visual_line_selected(p) {
            theme().select_bg
        } else {
            base_bg
        };
        let sign = if self.in_range(p) {
            Span::styled("▌", Style::default().fg(theme().blue))
        } else if self.annotated(p) {
            Span::styled("▌", Style::default().fg(theme().comment))
        } else {
            Span::raw(" ")
        };
        let old = l.old.map_or("    ".into(), |v| format!("{v:>4}"));
        let new = l.new.map_or("    ".into(), |v| format!("{v:>4}"));
        let marker = match l.kind {
            LineKind::Add => Style::default().fg(theme().green),
            LineKind::Remove => Style::default().fg(theme().red),
            _ => Style::default().fg(theme().muted),
        };
        let mut spans = vec![
            sign,
            Span::styled(format!("{old} {new} "), Style::default().fg(theme().muted)),
            Span::styled(format!("{} ", l.marker()), marker),
        ];
        if l.kind == LineKind::Hunk {
            let re = Regex::new(r"^(@@.*?@@)(.*)$").unwrap();
            if let Some(c) = re.captures(&l.text) {
                spans.push(Span::styled(
                    c[1].to_string(),
                    Style::default().fg(theme().blue),
                ));
                spans.push(Span::styled(
                    c[2].to_string(),
                    Style::default().fg(theme().muted),
                ));
            } else {
                spans.push(Span::styled(
                    l.text.clone(),
                    Style::default().fg(theme().blue),
                ));
            }
        } else if l.syntax.is_empty() {
            spans.push(Span::styled(
                l.text.clone(),
                Style::default().fg(theme().text),
            ));
        } else {
            for s in &l.syntax {
                spans.push(Span::styled(s.text.clone(), theme().syntax(s)));
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
    bottom_margin: usize,
) -> usize {
    let bottom_margin = bottom_margin.min(height.saturating_sub(1));
    while scroll < cursor {
        let occupied = rendered_rows_through(scroll, cursor, width, file, session, true);
        if occupied.saturating_add(bottom_margin) <= height {
            break;
        }
        scroll += 1;
    }
    scroll
}

fn editor_aware_scroll(
    height: usize,
    width: usize,
    file: &FileDiff,
    session: &Session,
    editor: &CommentEditor,
) -> usize {
    let Some(anchor) = editor.anchor.filter(|anchor| *anchor < file.lines.len()) else {
        return 0;
    };
    let mut scroll = anchor;
    let editor_rows = editor_visual_rows(&editor.text, editor_text_width(width))
        .len()
        .saturating_add(2);
    let desired_rows_before = height.saturating_sub(editor_rows) / 2;
    while scroll > 0 {
        let candidate = scroll - 1;
        if rendered_rows_through(candidate, anchor, width, file, session, false)
            > desired_rows_before
        {
            break;
        }
        scroll = candidate;
    }
    scroll
}

fn rendered_rows_through(
    scroll: usize,
    target: usize,
    width: usize,
    file: &FileDiff,
    session: &Session,
    sticky_hunk: bool,
) -> usize {
    if file.lines.is_empty() {
        return 0;
    }
    let target = target.min(file.lines.len() - 1);
    let scroll = scroll.min(target);
    let code_rows = file.lines[scroll..=target]
        .iter()
        .map(|line| {
            super::view_helpers::wrapped_code_row_count(&line.text, DIFF_PREFIX_WIDTH, width)
        })
        .sum::<usize>();
    let inline_rows = session
        .comments
        .iter()
        .filter(|comment| comment.path == file.path)
        .enumerate()
        .filter_map(|(number, comment)| {
            anchor_position(file, comment).and_then(|anchor| {
                (scroll <= anchor && anchor < target)
                    .then(|| inline_comment_lines(comment, number + 1, width).len())
            })
        })
        .sum::<usize>();
    let sticky_rows = if sticky_hunk && file.lines[scroll].kind != LineKind::Hunk {
        file.lines[..scroll]
            .iter()
            .rposition(|line| line.kind == LineKind::Hunk)
            .map(|position| {
                super::view_helpers::wrapped_code_row_count(
                    &file.lines[position].text,
                    DIFF_PREFIX_WIDTH,
                    width,
                )
            })
            .unwrap_or(0)
    } else {
        0
    };
    code_rows
        .saturating_add(inline_rows)
        .saturating_add(sticky_rows)
}

fn editor_text_width(width: usize) -> usize {
    width
        .saturating_sub(EDITOR_PREFIX_WIDTH + EDITOR_TEXT_INSET)
        .max(1)
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
            Style::default().bg(theme().comment_bg),
        ));
    }
    let mut spans = vec![Span::styled(
        " ".repeat(prefix_width.min(width)),
        Style::default().bg(theme().bg),
    )];
    spans.extend(card);
    Line::from(spans)
}
