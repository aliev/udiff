use crate::{
    comment::{Comment, CommentBody},
    highlight::highlight_source,
    model::{FileStatus, SyntaxSpan},
    theme::theme,
};
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub(super) fn file_status_spans(status: FileStatus) -> Vec<Span<'static>> {
    match status {
        FileStatus::Added => vec![Span::styled("+ ", Style::default().fg(theme().green))],
        FileStatus::Deleted => vec![Span::styled("− ", Style::default().fg(theme().red))],
        FileStatus::Modified => vec![Span::styled("~ ", Style::default().fg(theme().blue))],
        FileStatus::Renamed => vec![Span::styled("→ ", Style::default().fg(theme().blue))],
    }
}

pub(super) fn crop_spans(
    spans: Vec<Span<'static>>,
    offset: usize,
    width: usize,
) -> Vec<Span<'static>> {
    let mut skipped = 0;
    let mut visible = 0;
    let mut out = Vec::new();
    for span in spans {
        let mut text = String::new();
        for character in span.content.chars() {
            let character_width = UnicodeWidthChar::width(character).unwrap_or(0);
            if skipped + character_width <= offset {
                skipped += character_width;
                continue;
            }
            if visible + character_width > width {
                break;
            }
            text.push(character);
            visible += character_width;
        }
        if !text.is_empty() {
            out.push(Span::styled(text, span.style));
        }
        if visible >= width {
            break;
        }
    }
    out
}

/// `gutter` is how many columns the diff spends before its code starts, so the
/// card lines up with it. The unified and side-by-side views spend different
/// amounts.
pub(super) fn inline_comment_lines(
    comment: &Comment,
    number: usize,
    width: usize,
    gutter: usize,
) -> Vec<Line<'static>> {
    if let CommentBody::Suggestion { replacement } = &comment.body {
        return inline_suggestion_lines(comment, replacement, number, width, gutter);
    }

    let prefix_width = gutter.min(width);
    let card_width = width.saturating_sub(prefix_width);
    let label = format!("Comment #{number} · {}", comment.short_location());
    let wrapped = wrap_comment(comment.body.text(), card_width, &label);
    wrapped
        .into_iter()
        .enumerate()
        .map(|(index, text_line)| {
            let lead = if index == 0 {
                format!("┃ {label}  ")
            } else {
                "┃   ".into()
            };
            let mut card = crop_spans(
                vec![
                    Span::styled(
                        lead,
                        Style::default().fg(theme().comment).bg(theme().comment_bg),
                    ),
                    Span::styled(
                        text_line,
                        Style::default().fg(theme().text).bg(theme().comment_bg),
                    ),
                ],
                0,
                card_width,
            );
            let visible = card
                .iter()
                .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
                .sum::<usize>();
            if visible < card_width {
                card.push(Span::styled(
                    " ".repeat(card_width - visible),
                    Style::default().bg(theme().comment_bg),
                ));
            }
            let mut spans = vec![Span::styled(
                " ".repeat(prefix_width),
                Style::default().bg(theme().bg),
            )];
            spans.extend(card);
            Line::from(spans)
        })
        .collect()
}

fn inline_suggestion_lines(
    comment: &Comment,
    replacement: &str,
    number: usize,
    width: usize,
    gutter: usize,
) -> Vec<Line<'static>> {
    let prefix_width = gutter.min(width);
    let card_width = width.saturating_sub(prefix_width);
    let label = format!("Suggestion #{number} · {}", comment.short_location());
    let mut lines = vec![review_card_line(
        vec![Span::styled(
            format!("┃ {label}"),
            Style::default().fg(theme().green).bg(theme().green_bg),
        )],
        prefix_width,
        card_width,
        theme().green_bg,
    )];
    let available = card_width.saturating_sub(4).max(1);
    let highlighted = highlight_source(&comment.path, replacement);
    for (line_number, logical_line) in replacement.split('\n').enumerate() {
        let mut remaining = logical_line;
        let mut offset = 0;
        let mut first = true;
        loop {
            let (part, rest) = split_code_for_width(remaining, available);
            let lead = if first { "┃ + " } else { "┃   " };
            let mut code = highlighted
                .get(line_number)
                .map(|syntax| {
                    styled_syntax_spans(syntax, offset, offset + part.len(), theme().green_bg)
                })
                .unwrap_or_default();
            if code.is_empty() && !part.is_empty() {
                code.push(Span::styled(
                    part.to_owned(),
                    Style::default().fg(theme().text).bg(theme().green_bg),
                ));
            }
            let mut row = vec![Span::styled(
                lead,
                Style::default().fg(theme().green).bg(theme().green_bg),
            )];
            row.extend(code);
            lines.push(review_card_line(
                row,
                prefix_width,
                card_width,
                theme().green_bg,
            ));
            if rest.is_empty() {
                break;
            }
            offset += part.len();
            remaining = rest;
            first = false;
        }
    }
    lines
}

pub(super) fn styled_syntax_spans(
    syntax: &[SyntaxSpan],
    start: usize,
    end: usize,
    background: Color,
) -> Vec<Span<'static>> {
    let mut offset = 0;
    let mut output = Vec::new();
    for span in syntax {
        let span_end = offset + span.text.len();
        let overlap_start = start.max(offset);
        let overlap_end = end.min(span_end);
        if overlap_start < overlap_end {
            output.push(Span::styled(
                span.text[overlap_start - offset..overlap_end - offset].to_owned(),
                theme().syntax(span).bg(background),
            ));
        }
        offset = span_end;
        if offset >= end {
            break;
        }
    }
    output
}

fn review_card_line(
    spans: Vec<Span<'static>>,
    prefix_width: usize,
    card_width: usize,
    background: ratatui::style::Color,
) -> Line<'static> {
    let mut card = crop_spans(spans, 0, card_width);
    let visible = card
        .iter()
        .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
        .sum::<usize>();
    if visible < card_width {
        card.push(Span::styled(
            " ".repeat(card_width - visible),
            Style::default().bg(background),
        ));
    }
    let mut output = vec![Span::styled(
        " ".repeat(prefix_width),
        Style::default().bg(theme().bg),
    )];
    output.extend(card);
    Line::from(output)
}

fn split_code_for_width(text: &str, width: usize) -> (&str, &str) {
    if UnicodeWidthStr::width(text) <= width {
        return (text, "");
    }
    let mut used = 0;
    for (index, character) in text.char_indices() {
        let character_width = UnicodeWidthChar::width(character).unwrap_or(0);
        if used + character_width > width {
            let cut = if index == 0 {
                character.len_utf8()
            } else {
                index
            };
            return (&text[..cut], &text[cut..]);
        }
        used += character_width;
    }
    (text, "")
}

fn wrap_comment(text: &str, card_width: usize, label: &str) -> Vec<String> {
    let mut output = Vec::new();
    for logical_line in text.split('\n') {
        if logical_line.is_empty() {
            output.push(String::new());
            continue;
        }
        let mut remaining = logical_line;
        while !remaining.is_empty() {
            let lead = if output.is_empty() {
                format!("┃ {label}  ")
            } else {
                "┃   ".into()
            };
            let available = card_width
                .saturating_sub(UnicodeWidthStr::width(lead.as_str()))
                .max(1);
            let (line, rest) = split_for_width(remaining, available);
            output.push(line.to_owned());
            remaining = rest;
        }
    }
    if output.is_empty() {
        output.push(String::new());
    }
    output
}

fn split_for_width(text: &str, width: usize) -> (&str, &str) {
    if UnicodeWidthStr::width(text) <= width {
        return (text, "");
    }
    let mut used = 0;
    let mut last_space = None;
    let mut hard_cut = text.len();
    for (index, character) in text.char_indices() {
        let character_width = UnicodeWidthChar::width(character).unwrap_or(0);
        if used + character_width > width {
            hard_cut = index.max(character.len_utf8());
            break;
        }
        used += character_width;
        if character.is_whitespace() {
            last_space = Some(index);
        }
    }
    let cut = last_space.filter(|cut| *cut > 0).unwrap_or(hard_cut);
    (text[..cut].trim_end(), text[cut..].trim_start())
}
