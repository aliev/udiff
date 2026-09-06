use crate::model::{FileDiff, LineKind, SyntaxSpan};
use std::sync::OnceLock;
use syntect::{
    easy::HighlightLines,
    highlighting::{Style, Theme, ThemeSet},
    parsing::{SyntaxReference, SyntaxSet},
};

static SYNTAXES: OnceLock<SyntaxSet> = OnceLock::new();
static THEME: OnceLock<Theme> = OnceLock::new();

pub(crate) fn apply(file: &mut FileDiff) {
    let (syntaxes, syntax, theme) = resources(&file.path);
    let mut old = HighlightLines::new(syntax, theme);
    let mut new = HighlightLines::new(syntax, theme);
    for line in &mut file.lines {
        if line.kind == LineKind::Hunk {
            old = HighlightLines::new(syntax, theme);
            new = HighlightLines::new(syntax, theme);
            continue;
        }
        let source = format!("{}\n", line.text);
        let ranges = match line.kind {
            LineKind::Remove => old.highlight_line(&source, syntaxes),
            LineKind::Add => new.highlight_line(&source, syntaxes),
            LineKind::Context => {
                let _ = old.highlight_line(&source, syntaxes);
                new.highlight_line(&source, syntaxes)
            }
            _ => continue,
        };
        if let Ok(ranges) = ranges {
            line.syntax = syntax_spans(ranges);
        }
    }
}

pub(crate) fn highlight_source(path: &str, source: &str) -> Vec<Vec<SyntaxSpan>> {
    let (syntaxes, syntax, theme) = resources(path);
    let mut highlighter = HighlightLines::new(syntax, theme);
    source
        .split('\n')
        .map(|line| {
            let source = format!("{line}\n");
            let Ok(ranges) = highlighter.highlight_line(&source, syntaxes) else {
                return Vec::new();
            };
            syntax_spans(ranges)
        })
        .collect()
}

fn resources(path: &str) -> (&'static SyntaxSet, &'static SyntaxReference, &'static Theme) {
    let syntaxes = SYNTAXES.get_or_init(SyntaxSet::load_defaults_newlines);
    let syntax = syntaxes
        .find_syntax_for_file(path)
        .ok()
        .flatten()
        .unwrap_or_else(|| syntaxes.find_syntax_plain_text());
    let theme = THEME.get_or_init(|| {
        let themes = ThemeSet::load_defaults();
        let wanted = crate::theme::theme().syntect_theme;
        themes
            .themes
            .get(wanted)
            .or_else(|| themes.themes.values().next())
            .expect("syntect includes at least one default theme")
            .clone()
    });
    (syntaxes, syntax, theme)
}

fn syntax_spans(ranges: Vec<(Style, &str)>) -> Vec<SyntaxSpan> {
    let mut spans = ranges
        .into_iter()
        .map(|(style, text)| SyntaxSpan {
            text: text.into(),
            rgb: (style.foreground.r, style.foreground.g, style.foreground.b),
            bold: style
                .font_style
                .contains(syntect::highlighting::FontStyle::BOLD),
            italic: style
                .font_style
                .contains(syntect::highlighting::FontStyle::ITALIC),
        })
        .collect::<Vec<_>>();
    if let Some(last) = spans.last_mut()
        && last.text.ends_with('\n')
    {
        last.text.pop();
    }
    spans.retain(|span| !span.text.is_empty());
    spans
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{Mode, Palette};

    #[test]
    fn every_mode_names_a_syntect_theme_that_actually_loads() {
        let themes = ThemeSet::load_defaults();
        for mode in [Mode::Dark, Mode::Light, Mode::Mono] {
            let wanted = Palette::for_mode(mode).syntect_theme;
            assert!(
                themes.themes.contains_key(wanted),
                "{wanted:?} is missing, so this mode would silently fall back \
                 to an arbitrary theme"
            );
        }
    }
}
