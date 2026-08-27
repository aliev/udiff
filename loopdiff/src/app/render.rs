use super::{BG, BLUE, COMMENT, COMMENT_BG, GREEN, RED, TEXT};
use crate::model::FileStatus;
use ratatui::{
    style::Style,
    text::{Line, Span},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub(super) fn file_status_spans(status: FileStatus) -> Vec<Span<'static>> {
    match status {
        FileStatus::Added => vec![Span::styled("+ ", Style::default().fg(GREEN))],
        FileStatus::Deleted => vec![Span::styled("- ", Style::default().fg(RED))],
        FileStatus::Modified => vec![
            Span::styled("+", Style::default().fg(GREEN)),
            Span::styled("-", Style::default().fg(RED)),
        ],
        FileStatus::Renamed => vec![Span::styled("R ", Style::default().fg(BLUE))],
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

pub(super) fn inline_message_lines(text: &str, width: usize) -> Vec<Line<'static>> {
    const CODE_COLUMN: usize = 13;
    let prefix_width = CODE_COLUMN.min(width);
    let card_width = width.saturating_sub(prefix_width);
    let label = "Comment";
    let wrapped = wrap_comment(text, card_width, label);
    wrapped
        .into_iter()
        .enumerate()
        .map(|(index, text_line)| {
            let lead = if index == 0 {
                format!("┃ {label} · ")
            } else {
                "┃   ".into()
            };
            let mut card = crop_spans(
                vec![
                    Span::styled(lead, Style::default().fg(COMMENT).bg(COMMENT_BG)),
                    Span::styled(text_line, Style::default().fg(TEXT).bg(COMMENT_BG)),
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
                    Style::default().bg(COMMENT_BG),
                ));
            }
            let mut spans = vec![Span::styled(
                " ".repeat(prefix_width),
                Style::default().bg(BG),
            )];
            spans.extend(card);
            Line::from(spans)
        })
        .collect()
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
                format!("┃ {label} · ")
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
