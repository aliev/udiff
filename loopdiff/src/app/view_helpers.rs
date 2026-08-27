use super::{BG, SELECT_BG, TAB_WIDTH, TEXT, editor::next_boundary};
use crate::{
    comment::Comment,
    model::{DiffLine, FileDiff, LineKind},
};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
pub(super) fn ordered(first: usize, second: usize) -> (usize, usize) {
    (first.min(second), first.max(second))
}

pub(super) fn ordered_position(
    first: (usize, usize),
    second: (usize, usize),
) -> ((usize, usize), (usize, usize)) {
    if first <= second {
        (first, second)
    } else {
        (second, first)
    }
}

pub(super) fn apply_block_cursor<'a>(
    spans: &mut Vec<Span<'a>>,
    code_start: usize,
    column: usize,
    background: Color,
) {
    let mut offset = 0;
    for index in code_start..spans.len() {
        let length = spans[index].content.chars().count();
        if column < offset + length {
            let local = column - offset;
            let content = spans[index].content.to_string();
            let style = spans[index].style;
            let before = content.chars().take(local).collect::<String>();
            let cursor = content.chars().nth(local).unwrap().to_string();
            let after = content.chars().skip(local + 1).collect::<String>();
            spans.splice(
                index..=index,
                [
                    Span::styled(before, style),
                    Span::styled(
                        cursor,
                        Style::default()
                            .fg(background)
                            .bg(TEXT)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(after, style),
                ],
            );
            return;
        }
        offset += length;
    }
    if column == 0 && offset == 0 {
        spans.push(Span::styled(
            " ",
            Style::default()
                .fg(background)
                .bg(TEXT)
                .add_modifier(Modifier::BOLD),
        ));
    }
}

pub(super) fn expand_tabs(spans: &mut [Span<'_>], code_start: usize) {
    let mut display_width = 0;
    for span in &mut spans[code_start..] {
        let mut expanded = String::new();
        for character in span.content.chars() {
            if character == '\t' {
                let spaces = TAB_WIDTH - display_width % TAB_WIDTH;
                expanded.push_str(&" ".repeat(spaces));
                display_width += spaces;
            } else {
                expanded.push(character);
                display_width += UnicodeWidthChar::width(character).unwrap_or(0);
            }
        }
        span.content = expanded.into();
    }
}

pub(super) fn expanded_character_column(text: &str, target: usize) -> usize {
    let mut character_column = 0;
    let mut display_width = 0;
    for (index, character) in text.chars().enumerate() {
        if index == target {
            break;
        }
        if character == '\t' {
            let spaces = TAB_WIDTH - display_width % TAB_WIDTH;
            character_column += spaces;
            display_width += spaces;
        } else {
            character_column += 1;
            display_width += UnicodeWidthChar::width(character).unwrap_or(0);
        }
    }
    character_column
}

pub(super) fn wrap_code_line(
    line: Line<'_>,
    code_start: usize,
    width: usize,
) -> Vec<Line<'static>> {
    let mut spans = line.spans.into_iter();
    let prefix = spans
        .by_ref()
        .take(code_start)
        .map(|span| Span::styled(span.content.into_owned(), span.style))
        .collect::<Vec<_>>();
    let prefix_width = prefix
        .iter()
        .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
        .sum::<usize>();
    let background = prefix.first().and_then(|span| span.style.bg).unwrap_or(BG);
    let available = width.saturating_sub(prefix_width);
    if available == 0 {
        let mut unwrapped = prefix;
        unwrapped.extend(spans.map(|span| Span::styled(span.content.into_owned(), span.style)));
        return vec![Line::from(unwrapped)];
    }

    let mut code_rows = vec![Vec::new()];
    let mut used = 0;
    for span in spans {
        for character in span.content.chars() {
            let character_width = UnicodeWidthChar::width(character).unwrap_or(0);
            if used > 0 && used + character_width > available {
                code_rows.push(Vec::new());
                used = 0;
            }
            push_styled_character(code_rows.last_mut().unwrap(), character, span.style);
            used += character_width;
        }
    }

    code_rows
        .into_iter()
        .enumerate()
        .map(|(index, code)| {
            let mut row = if index == 0 {
                prefix.clone()
            } else {
                vec![Span::styled(
                    " ".repeat(prefix_width),
                    Style::default().bg(background),
                )]
            };
            row.extend(code);
            let rendered_width = row
                .iter()
                .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
                .sum::<usize>();
            if rendered_width < width {
                row.push(Span::styled(
                    " ".repeat(width - rendered_width),
                    Style::default().bg(background),
                ));
            }
            Line::from(row)
        })
        .collect()
}

pub(super) fn wrapped_scroll(
    scroll: usize,
    cursor: usize,
    height: usize,
    width: usize,
    prefix_width: usize,
    lines: &[DiffLine],
    sticky_hunk: bool,
) -> usize {
    let cursor = cursor.min(lines.len().saturating_sub(1));
    let mut scroll = scroll.min(cursor);
    while scroll < cursor {
        let content_rows = lines[scroll..=cursor]
            .iter()
            .map(|line| wrapped_code_row_count(&line.text, prefix_width, width))
            .sum::<usize>();
        let sticky_rows = if sticky_hunk && lines[scroll].kind != LineKind::Hunk {
            lines[..scroll]
                .iter()
                .rposition(|line| line.kind == LineKind::Hunk)
                .map(|position| wrapped_code_row_count(&lines[position].text, prefix_width, width))
                .unwrap_or(0)
        } else {
            0
        };
        if content_rows.saturating_add(sticky_rows) <= height {
            break;
        }
        scroll += 1;
    }
    scroll
}

pub(super) fn wrapped_code_row_count(text: &str, prefix_width: usize, width: usize) -> usize {
    let available = width.saturating_sub(prefix_width);
    if available == 0 {
        return 1;
    }
    let mut rows = 1;
    let mut used = 0;
    let mut display_width = 0;
    for character in text.chars() {
        if character == '\t' {
            let spaces = TAB_WIDTH - display_width % TAB_WIDTH;
            for _ in 0..spaces {
                if used == available {
                    rows += 1;
                    used = 0;
                }
                used += 1;
            }
            display_width += spaces;
        } else {
            let character_width = UnicodeWidthChar::width(character).unwrap_or(0);
            if used > 0 && used + character_width > available {
                rows += 1;
                used = 0;
            }
            used += character_width;
            display_width += character_width;
        }
    }
    rows
}

pub(super) fn push_styled_character(spans: &mut Vec<Span<'static>>, character: char, style: Style) {
    if let Some(last) = spans.last_mut()
        && last.style == style
    {
        last.content.to_mut().push(character);
    } else {
        spans.push(Span::styled(character.to_string(), style));
    }
}

pub(super) fn apply_character_selection<'a>(
    spans: &mut Vec<Span<'a>>,
    code_start: usize,
    start: usize,
    end: usize,
    cursor: Option<usize>,
) {
    let original = std::mem::take(spans);
    let mut rebuilt = Vec::new();
    let mut column = 0;
    for (index, span) in original.into_iter().enumerate() {
        if index < code_start {
            rebuilt.push(span);
            continue;
        }
        for character in span.content.chars() {
            let style = if cursor == Some(column) {
                Style::default()
                    .fg(BG)
                    .bg(TEXT)
                    .add_modifier(Modifier::BOLD)
            } else if (start..=end).contains(&column) {
                span.style.bg(SELECT_BG)
            } else {
                span.style
            };
            rebuilt.push(Span::styled(character.to_string(), style));
            column += 1;
        }
    }
    *spans = rebuilt;
}

pub(super) fn line_in_note(l: &DiffLine, n: &Comment) -> bool {
    if l.kind == LineKind::Hunk {
        return n.excerpt.lines().any(|x| x.trim() == l.text);
    }
    (l.old
        .zip(n.old_start)
        .is_some_and(|(x, s)| s <= x && x <= n.old_end.unwrap_or(s)))
        || (l
            .new
            .zip(n.new_start)
            .is_some_and(|(x, s)| s <= x && x <= n.new_end.unwrap_or(s)))
}
pub(super) fn anchor_position(f: &FileDiff, n: &Comment) -> Option<usize> {
    f.lines
        .iter()
        .position(|l| {
            (n.anchor_old.is_none() || l.old == n.anchor_old)
                && (n.anchor_new.is_none() || l.new == n.anchor_new)
                && line_in_note(l, n)
        })
        .or_else(|| f.lines.iter().position(|l| line_in_note(l, n)))
}
pub(super) fn editor_visual_rows(text: &str, width: usize) -> Vec<(usize, usize)> {
    let mut rows = Vec::new();
    let mut line_start = 0;
    loop {
        let line_end = text[line_start..]
            .find('\n')
            .map_or(text.len(), |offset| line_start + offset);
        if line_start == line_end {
            rows.push((line_start, line_end));
        } else {
            let mut start = line_start;
            while start < line_end {
                let mut used = 0;
                let mut last_space_end = None;
                let mut hard_cut = line_end;
                let mut overflowed = false;
                for (offset, character) in text[start..line_end].char_indices() {
                    let character_width = UnicodeWidthChar::width(character).unwrap_or(0);
                    if used + character_width > width {
                        hard_cut = start + offset;
                        overflowed = true;
                        break;
                    }
                    used += character_width;
                    if character.is_whitespace() {
                        last_space_end = Some(start + offset + character.len_utf8());
                    }
                }
                if !overflowed {
                    rows.push((start, line_end));
                    break;
                }
                let cut = last_space_end
                    .filter(|cut| *cut > start)
                    .unwrap_or_else(|| {
                        hard_cut.max(next_boundary(text, start).unwrap_or(line_end))
                    });
                rows.push((start, cut));
                start = cut;
            }
        }
        if line_end == text.len() {
            break;
        }
        line_start = line_end + 1;
    }
    rows
}
