use super::{
    Focus,
    comment_editor::{CommentEditor, Mode as EditorMode},
    diff_pane::{DiffPane, VisualMode},
    render::{inline_comment_lines, styled_syntax_spans},
    rows::{Row, Side},
    session::Session,
    view_helpers::{
        Metrics, anchor_position, apply_block_cursor, apply_character_selection, crop_code_line,
        editor_visual_rows, expand_tabs, expanded_character_column, line_in_comment, ordered,
        ordered_position, reverse_row, wrap_code_line, wrapped_scroll,
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
    widgets::{Block, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
};
use regex::Regex;
use unicode_width::UnicodeWidthStr;

const DIFF_PREFIX_WIDTH: usize = 13;
/// Gutter column of the change marker, reused for the soft-wrap marker so a
/// continuation row lines up with the `+`/`-` above it.
const WRAP_MARKER_COLUMN: usize = 11;
/// Review mark, four digits, change marker, and the spaces between them: what
/// one side of a paired row spends before its code starts.
const SPLIT_PREFIX_WIDTH: usize = 8;
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
            sidebar_hidden: false,
        }
        .diff_line(line, position, width, None)
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
            sidebar_hidden: false,
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
    sidebar_hidden: bool,
) {
    Renderer {
        pane,
        session,
        editor,
        focus,
        sidebar_hidden,
    }
    .draw_main(frame, area);
}

struct Renderer<'a> {
    pane: &'a mut DiffPane,
    session: &'a Session,
    editor: &'a CommentEditor,
    focus: Focus,
    sidebar_hidden: bool,
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
            .constraints([Constraint::Length(1), Constraint::Min(3)])
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
        // With the explorer away, a mark where it used to be says it is folded
        // rather than gone.
        let mut left_header = vec![if self.sidebar_hidden {
            Span::styled("\u{25b8}", Style::default().fg(theme().border))
        } else {
            Span::raw(" ")
        }];
        left_header.extend(crate::app::render::file_status_spans(file.status));
        left_header.extend([
            Span::styled(
                shown_path.clone(),
                Style::default()
                    .fg(theme().text)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  +{}", file.additions()),
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
                theme().chip(if reviewed {
                    theme().green
                } else {
                    theme().blue
                }),
            ),
            Span::raw(" "),
        ];
        f.render_widget(
            Block::default().style(Style::default().bg(theme().surface)),
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
            .split(parts[0]);
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

    /// Rows for one diff line: several when it folds, exactly one when it is
    /// cut at the right instead.
    fn rows_for(&self, line: Line<'_>, width: usize, kind: LineKind) -> Vec<Line<'static>> {
        if self.pane.wrap {
            return wrap_code_line(line, 3, width, Some(WRAP_MARKER_COLUMN));
        }
        // A hunk header labels the code rather than being code, and it is
        // shorter than the code it labels, so scrolling would simply lose it.
        let offset = if kind == LineKind::Hunk {
            0
        } else {
            self.pane.h_scroll
        };
        vec![crop_code_line(line, 3, width, offset)]
    }

    /// Keeps the cursor's column in view the way `ensure_visible` keeps its
    /// line in view. Nothing scrolls sideways while lines fold.
    fn ensure_column_visible(&mut self, width: usize) {
        if self.pane.wrap {
            self.pane.h_scroll = 0;
            return;
        }
        // Side by side, the cursor lives in half a pane behind a narrower
        // gutter; measuring the whole pane would let it walk off the edge.
        let available = if self.pane.split {
            (width.saturating_sub(1) / 2).saturating_sub(SPLIT_PREFIX_WIDTH)
        } else {
            width.saturating_sub(DIFF_PREFIX_WIDTH)
        }
        .max(1);
        let line = &self.active_lines()[self.pane.cursor];
        let column = expanded_character_column(&line.text, self.pane.visual_col);
        if column < self.pane.h_scroll {
            self.pane.h_scroll = column;
        } else if column >= self.pane.h_scroll + available {
            self.pane.h_scroll = column + 1 - available;
        }
    }

    /// One side of a paired row, already cropped to its half. An empty cell is
    /// drawn as background so the two sides stay level.
    fn split_cell(&self, line: Option<usize>, side: Side, width: usize) -> Vec<Span<'static>> {
        let Some(index) = line else {
            return vec![Span::styled(
                " ".repeat(width),
                Style::default().bg(theme().bg),
            )];
        };
        let file = self.current();
        let built = self.diff_line(&file.lines[index], index, width, Some(side));
        crop_code_line(built, 3, width, self.pane.h_scroll).spans
    }

    /// A paired row is one line across the whole pane, which is what lets a
    /// hunk header and a comment card cross it.
    fn split_row(&self, row: Row, width: usize) -> Line<'static> {
        let left_width = width.saturating_sub(1) / 2;
        let right_width = width.saturating_sub(1 + left_width);
        let mut spans = self.split_cell(row.left, Side::Left, left_width);
        spans.push(Span::styled(
            "\u{2502}",
            Style::default().fg(theme().border).bg(theme().bg),
        ));
        spans.extend(self.split_cell(row.right, Side::Right, right_width));
        Line::from(spans)
    }

    /// What the editor card calls itself. Both views show the same thing.
    fn editor_title(&self) -> String {
        let kind = if self.editor.mode == EditorMode::Suggestion {
            "SUGGESTION"
        } else {
            "COMMENT"
        };
        if let Some(key) = &self.editor.editing_key {
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
        }
    }

    /// Indents a full-width card so it sits under the pane it belongs to.
    /// A note on a right-hand line drawn from the far left, under an empty
    /// left pane, reads as belonging to nothing.
    fn under_side(&self, card: Vec<Line<'static>>, side: Side, width: usize) -> Vec<Line<'static>> {
        let left_width = width.saturating_sub(1) / 2;
        if side == Side::Left {
            return card;
        }
        card.into_iter()
            .map(|line| {
                let mut spans = vec![Span::styled(
                    " ".repeat(left_width + 1),
                    Style::default().bg(theme().bg),
                )];
                spans.extend(line.spans);
                Line::from(spans)
            })
            .collect()
    }

    fn draw_diff(&mut self, f: &mut Frame, a: Rect) {
        let height = a.height as usize;
        self.ensure_visible(height);
        self.ensure_column_visible(a.width as usize);
        let mut lines = Vec::new();
        let mut map = Vec::new();
        let file = self.current().clone();
        let editor_active = self.focus == Focus::Editor && self.editor.anchor.is_some();
        let sticky_hunk = !editor_active;
        if editor_active {
            self.pane.scroll = editor_aware_scroll(
                height,
                a.width as usize,
                &file,
                self.session,
                self.editor,
                self.pane.wrap,
            );
        } else {
            let metrics = Metrics {
                width: a.width as usize,
                prefix_width: DIFF_PREFIX_WIDTH,
                wrap: self.pane.wrap,
            };
            self.pane.scroll = wrapped_scroll(
                self.pane.scroll,
                self.pane.cursor,
                height,
                metrics,
                &file.lines,
                sticky_hunk,
            );
            self.pane.scroll = review_aware_scroll(
                self.pane.scroll,
                self.pane.cursor,
                height,
                metrics,
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
            for line in self.rows_for(
                self.diff_line(&file.lines[sticky], sticky, a.width as usize, None),
                a.width as usize,
                LineKind::Hunk,
            ) {
                lines.push(line);
                map.push(Some(sticky));
            }
        }
        if self.pane.split {
            // Row is Copy and there is one per line, so the copy buys a
            // shorter borrow of the pane for the cost of a small vector.
            let rows = self.pane.rows.clone();
            // `scroll` counts rows here, not lines: pairing merges a run of
            // removals with the additions that replaced it, so the two spaces
            // drift apart as soon as a file holds one.
            let cursor_row = rows
                .iter()
                .position(|row| row.holds(self.pane.cursor))
                .unwrap_or(0);
            let editor_row = self
                .editor
                .anchor
                .and_then(|anchor| rows.iter().position(|row| row.holds(anchor)));
            let focus_row = if editor_active {
                editor_row.unwrap_or(cursor_row)
            } else {
                cursor_row
            };
            self.pane.scroll = self.pane.scroll.min(focus_row);
            if focus_row >= self.pane.scroll + height.saturating_sub(1) {
                self.pane.scroll = focus_row + 2 - height.max(2);
            }
            for row in rows.iter().skip(self.pane.scroll) {
                if lines.len() >= height {
                    break;
                }
                let anchor = if row.full_width {
                    row.left
                } else {
                    row.occupant(self.pane.side).or(row.left).or(row.right)
                };
                if row.full_width {
                    let index = row.left.unwrap_or(0);
                    let built = self.diff_line(&file.lines[index], index, a.width as usize, None);
                    lines.push(crop_code_line(built, 3, a.width as usize, 0));
                } else {
                    lines.push(self.split_row(*row, a.width as usize));
                }
                map.push(anchor);
                let card_width = a.width as usize / 2;
                if editor_active
                    && let Some(open) = self.editor.anchor
                    && row.holds(open)
                {
                    let side = if row.right == Some(open) && row.left != Some(open) {
                        Side::Right
                    } else {
                        Side::Left
                    };
                    let mut card = Vec::new();
                    let mut ignored = Vec::new();
                    let title = self.editor_title();
                    self.append_editor(&mut card, &mut ignored, &title, card_width);
                    for line in self.under_side(card, side, a.width as usize) {
                        lines.push(line);
                        map.push(None);
                    }
                }
                for (number, comment) in self
                    .session
                    .comments
                    .iter()
                    .filter(|comment| comment.path == file.path)
                    .enumerate()
                    // Either occupant of the row can carry the comment, not
                    // just the one the cursor happens to be beside.
                    .filter(|(_, comment)| {
                        anchor_position(&file, comment).is_some_and(|line| row.holds(line))
                    })
                {
                    let line = anchor_position(&file, comment);
                    let side = if row.right == line && row.left != line {
                        Side::Right
                    } else {
                        Side::Left
                    };
                    let card = inline_comment_lines(comment, number + 1, card_width);
                    for comment_line in self.under_side(card, side, a.width as usize) {
                        lines.push(comment_line);
                        map.push(None);
                    }
                }
            }
            self.pane.row_map = map;
            f.render_widget(
                Paragraph::new(lines).style(Style::default().fg(theme().text).bg(theme().bg)),
                a,
            );
            return;
        }
        for p in self.pane.scroll..file.lines.len() {
            if lines.len() >= height {
                break;
            }
            let l = &file.lines[p];
            for line in self.rows_for(
                self.diff_line(l, p, a.width as usize, None),
                a.width as usize,
                l.kind,
            ) {
                lines.push(line);
                map.push(Some(p));
            }
            let editor_here = Some(p) == self.editor.anchor && self.focus == Focus::Editor;
            if editor_here {
                let title = self.editor_title();
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
            Paragraph::new(lines).style(Style::default().fg(theme().text).bg(theme().bg)),
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

    /// `side` picks whose line number the gutter carries: `None` for the
    /// unified view, which shows both.
    fn diff_line<'a>(
        &self,
        l: &'a DiffLine,
        p: usize,
        width: usize,
        side: Option<Side>,
    ) -> Line<'a> {
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
        let number = |value: Option<u32>| value.map_or("    ".to_owned(), |v| format!("{v:>4}"));
        let numbers = match side {
            None => format!("{} {} ", number(l.old), number(l.new)),
            Some(Side::Left) => format!("{} ", number(l.old)),
            Some(Side::Right) => format!("{} ", number(l.new)),
        };
        let marker = match l.kind {
            LineKind::Add => Style::default().fg(theme().green),
            LineKind::Remove => Style::default().fg(theme().red),
            _ => Style::default().fg(theme().muted),
        };
        let mut spans = vec![
            sign,
            Span::styled(numbers, Style::default().fg(theme().muted)),
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
            let cursor = (p == self.pane.cursor
                && side.is_none_or(|drawn| drawn == self.pane.side))
            .then(|| {
                expanded_character_column(&l.text, self.pane.visual_col.clamp(raw_start, raw_end))
            });
            let start = expanded_character_column(&l.text, raw_start);
            let end = expanded_character_column(&l.text, raw_end + 1).saturating_sub(1);
            apply_character_selection(&mut spans, 3, start, end, cursor);
        } else if p == self.pane.cursor
            && self.pane.visual_mode.is_none()
            && self.focus == Focus::Diff
            // A context line sits in both panes under the same index, so the
            // side has to agree before the cursor is drawn on it.
            && side.is_none_or(|drawn| drawn == self.pane.side)
        {
            apply_block_cursor(
                &mut spans,
                3,
                expanded_character_column(&l.text, self.pane.visual_col),
                code_bg,
            );
        }
        // Visual-line selection is carried as a bare colour, which monochrome
        // does not have, so the whole row is reversed instead.
        if theme().monochrome && self.visual_line_selected(p) {
            reverse_row(&mut spans);
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
    metrics: Metrics,
    file: &FileDiff,
    session: &Session,
    bottom_margin: usize,
) -> usize {
    let (width, wrap) = (metrics.width, metrics.wrap);
    let bottom_margin = bottom_margin.min(height.saturating_sub(1));
    while scroll < cursor {
        let occupied = rendered_rows_through(scroll, cursor, width, file, session, true, wrap);
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
    wrap: bool,
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
        if rendered_rows_through(candidate, anchor, width, file, session, false, wrap)
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
    wrap: bool,
) -> usize {
    if file.lines.is_empty() {
        return 0;
    }
    let target = target.min(file.lines.len() - 1);
    let scroll = scroll.min(target);
    let code_rows = file.lines[scroll..=target]
        .iter()
        .map(|line| {
            super::view_helpers::wrapped_code_row_count(&line.text, DIFF_PREFIX_WIDTH, width, wrap)
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
                    wrap,
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
