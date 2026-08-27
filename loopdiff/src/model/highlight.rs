use super::{FileDiff, LineKind, SyntaxSpan};
use std::sync::OnceLock;
use syntect::{
    easy::HighlightLines,
    highlighting::{Theme, ThemeSet},
    parsing::SyntaxSet,
};

static SYNTAXES: OnceLock<SyntaxSet> = OnceLock::new();
static THEME: OnceLock<Theme> = OnceLock::new();

pub(super) fn apply(file: &mut FileDiff) {
    let syntaxes = SYNTAXES.get_or_init(SyntaxSet::load_defaults_newlines);
    let Some(syntax) = syntaxes
        .find_syntax_for_file(&file.path)
        .ok()
        .flatten()
        .or_else(|| syntaxes.find_syntax_plain_text().into())
    else {
        return;
    };
    let theme = THEME.get_or_init(|| {
        let themes = ThemeSet::load_defaults();
        themes
            .themes
            .get("base16-ocean.dark")
            .or_else(|| themes.themes.values().next())
            .expect("syntect includes at least one default theme")
            .clone()
    });
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
            line.syntax = spans;
        }
    }
}
