//! Appearance modes. One palette is resolved from the environment at startup
//! and every colour in the interface comes from it.

use ratatui::style::{Color, Modifier, Style};
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Mode {
    Dark,
    Light,
    Mono,
}

/// Which appearance the environment asks for. Pure so the rules can be tested
/// without touching the real environment.
pub(crate) fn resolve(
    no_color: Option<&str>,
    udiff_theme: Option<&str>,
    queried_background: Option<(u8, u8, u8)>,
    colorfgbg: Option<&str>,
) -> Mode {
    // https://no-color.org: any non-empty value suppresses colour.
    if no_color.is_some_and(|value| !value.is_empty()) {
        return Mode::Mono;
    }
    match udiff_theme.map(str::trim) {
        Some("dark") => return Mode::Dark,
        Some("light") => return Mode::Light,
        Some("mono") => return Mode::Mono,
        // An unreadable value is ignored rather than fatal: a typo in a shell
        // profile must not stop a review.
        _ => {}
    }
    // What the terminal itself just answered outranks COLORFGBG, which is a
    // guess left in the environment — sometimes by a different terminal, and
    // usually before the last time the theme changed.
    if let Some(background) = queried_background {
        return if crate::terminal_background::is_light(background) {
            Mode::Light
        } else {
            Mode::Dark
        };
    }
    // COLORFGBG is "foreground;background", sometimes with a middle field.
    // Colour indexes 7 and 15 are the light backgrounds.
    let background = colorfgbg
        .and_then(|value| value.rsplit(';').next())
        .and_then(|last| last.trim().parse::<u8>().ok());
    match background {
        Some(7 | 15) => Mode::Light,
        _ => Mode::Dark,
    }
}

pub(crate) struct Palette {
    pub(crate) bg: Color,
    pub(crate) surface: Color,
    pub(crate) border: Color,
    pub(crate) text: Color,
    pub(crate) muted: Color,
    pub(crate) blue: Color,
    pub(crate) green: Color,
    pub(crate) green_bg: Color,
    pub(crate) red: Color,
    pub(crate) red_bg: Color,
    /// The gutter's own trio. A solid cell of `red`, `green` or `comment` —
    /// the colours of a `-`, a `+` or a note's rule — reads far louder than
    /// the pale rows those markers sit on, so the gutter looked like a
    /// different palette, worst in the light theme. These are the same hues
    /// pulled three quarters of the way from the marker towards the row it
    /// belongs to: soft enough to match the diff, and still 3.09:1 or better
    /// against the pane, which is the floor for something that is not text.
    pub(crate) map_removed: Color,
    pub(crate) map_added: Color,
    pub(crate) map_noted: Color,
    pub(crate) hunk_bg: Color,
    pub(crate) comment: Color,
    pub(crate) comment_bg: Color,
    pub(crate) select_bg: Color,
    /// Name of the syntect theme whose colours suit this palette.
    pub(crate) syntect_theme: &'static str,
    /// Whether colour carries no meaning, so emphasis has to use attributes.
    pub(crate) monochrome: bool,
}

impl Palette {
    /// The cell under the cursor. Underlines in monochrome, because a reversed
    /// cursor inside a reversed visual-line row would merge into the row.
    pub(crate) fn cursor(&self, background: Color) -> Style {
        if self.monochrome {
            Style::default().add_modifier(Modifier::UNDERLINED | Modifier::BOLD)
        } else {
            Style::default()
                .fg(background)
                .bg(self.text)
                .add_modifier(Modifier::BOLD)
        }
    }

    /// A selected row or run of characters, layered over whatever style the
    /// content already carries.
    pub(crate) fn selected(&self, base: Style) -> Style {
        if self.monochrome {
            base.add_modifier(Modifier::REVERSED)
        } else {
            base.bg(self.select_bg)
        }
    }

    /// A small inverted label: the mode badge, a help key, the file counter.
    pub(crate) fn chip(&self, foreground: Color) -> Style {
        if self.monochrome {
            Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD)
        } else {
            Style::default()
                .fg(foreground)
                .bg(self.select_bg)
                .add_modifier(Modifier::BOLD)
        }
    }

    /// The block caret in the filter prompt.
    pub(crate) fn caret(&self) -> Style {
        if self.monochrome {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default().bg(self.text)
        }
    }

    /// Syntect writes its own RGB into every span, which is a second source of
    /// colour. Monochrome drops it and keeps the weight.
    pub(crate) fn syntax(&self, span: &crate::model::SyntaxSpan) -> Style {
        let mut style = Style::default();
        if !self.monochrome {
            style = style.fg(Color::Rgb(span.rgb.0, span.rgb.1, span.rgb.2));
        }
        if span.bold {
            style = style.add_modifier(Modifier::BOLD);
        }
        if span.italic {
            style = style.add_modifier(Modifier::ITALIC);
        }
        style
    }

    pub(crate) fn for_mode(mode: Mode) -> Self {
        match mode {
            Mode::Dark => Self::dark(),
            Mode::Light => Self::light(),
            Mode::Mono => Self::mono(),
        }
    }

    fn dark() -> Self {
        Self {
            bg: Color::Rgb(15, 18, 25),
            surface: Color::Rgb(21, 25, 35),
            border: Color::Rgb(45, 51, 66),
            text: Color::Rgb(220, 224, 232),
            muted: Color::Rgb(122, 132, 153),
            blue: Color::Rgb(122, 162, 247),
            green: Color::Rgb(158, 206, 106),
            green_bg: Color::Rgb(24, 45, 35),
            red: Color::Rgb(247, 118, 142),
            red_bg: Color::Rgb(54, 31, 40),
            map_removed: Color::Rgb(197, 95, 115),
            map_added: Color::Rgb(123, 164, 88),
            map_noted: Color::Rgb(178, 140, 84),
            hunk_bg: Color::Rgb(28, 38, 58),
            comment: Color::Rgb(224, 175, 104),
            comment_bg: Color::Rgb(47, 39, 28),
            select_bg: Color::Rgb(38, 49, 70),
            syntect_theme: "base16-ocean.dark",
            monochrome: false,
        }
    }

    fn light() -> Self {
        Self {
            bg: Color::Rgb(255, 255, 255),
            // Deeper than Primer's canvas.subtle: the same luminance step
            // reads far weaker at the top of the range than it does in the
            // dark palette, and the panels stopped reading as panels.
            surface: Color::Rgb(237, 241, 245),
            border: Color::Rgb(208, 215, 222),
            text: Color::Rgb(31, 35, 40),
            muted: Color::Rgb(101, 109, 118),
            blue: Color::Rgb(9, 105, 218),
            green: Color::Rgb(26, 127, 55),
            green_bg: Color::Rgb(218, 251, 225),
            red: Color::Rgb(207, 34, 46),
            red_bg: Color::Rgb(255, 235, 233),
            map_removed: Color::Rgb(219, 86, 95),
            map_added: Color::Rgb(76, 159, 99),
            map_noted: Color::Rgb(180, 141, 51),
            hunk_bg: Color::Rgb(221, 244, 255),
            comment: Color::Rgb(154, 103, 0),
            comment_bg: Color::Rgb(255, 248, 197),
            select_bg: Color::Rgb(221, 232, 244),
            syntect_theme: "InspiredGitHub",
            monochrome: false,
        }
    }

    fn mono() -> Self {
        Self {
            bg: Color::Reset,
            surface: Color::Reset,
            border: Color::Reset,
            text: Color::Reset,
            muted: Color::Reset,
            blue: Color::Reset,
            green: Color::Reset,
            green_bg: Color::Reset,
            red: Color::Reset,
            red_bg: Color::Reset,
            map_removed: Color::Reset,
            map_added: Color::Reset,
            map_noted: Color::Reset,
            hunk_bg: Color::Reset,
            comment: Color::Reset,
            comment_bg: Color::Reset,
            select_bg: Color::Reset,
            // Monochrome never reaches syntect's colours, so the name only has
            // to be one that loads.
            syntect_theme: "base16-ocean.dark",
            monochrome: true,
        }
    }

    fn from_environment(queried_background: Option<(u8, u8, u8)>) -> Self {
        let read = |name: &str| std::env::var(name).ok();
        Self::for_mode(resolve(
            read("NO_COLOR").as_deref(),
            read("UDIFF_THEME").as_deref(),
            queried_background,
            read("COLORFGBG").as_deref(),
        ))
    }
}

static PALETTE: OnceLock<Palette> = OnceLock::new();

/// Resolves the palette before anything can read it. Lazy initialisation would
/// reach the same answer, but an explicit call keeps the ordering visible.
pub(crate) fn init(queried_background: Option<(u8, u8, u8)>) {
    let _ = PALETTE.set(Palette::from_environment(queried_background));
}

pub(crate) fn theme() -> &'static Palette {
    PALETTE.get_or_init(|| {
        // The suite asserts exact colours, so it must not change behaviour when
        // a developer has UDIFF_THEME or NO_COLOR exported.
        if cfg!(test) {
            Palette::for_mode(Mode::Dark)
        } else {
            Palette::from_environment(None)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channels(colour: Color) -> (f64, f64, f64) {
        let Color::Rgb(r, g, b) = colour else {
            panic!("a coloured palette paints in rgb")
        };
        let linear = |v: u8| {
            let v = f64::from(v) / 255.0;
            if v <= 0.03928 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        (linear(r), linear(g), linear(b))
    }

    fn contrast(a: Color, b: Color) -> f64 {
        let luminance = |colour| {
            let (r, g, b) = channels(colour);
            0.2126 * r + 0.7152 * g + 0.0722 * b
        };
        let (high, low) = {
            let (a, b) = (luminance(a), luminance(b));
            (a.max(b), a.min(b))
        };
        (high + 0.05) / (low + 0.05)
    }

    #[test]
    fn the_map_stays_visible_while_it_matches_the_diff() {
        // The gutter's trio is pulled towards the pale rows the diff paints,
        // and pulling further would sink it into the pane. 3.0 is the floor
        // for a thing that is not text.
        for palette in [Palette::dark(), Palette::light()] {
            for mark in [palette.map_removed, palette.map_added, palette.map_noted] {
                let ratio = contrast(mark, palette.bg);
                assert!(ratio >= 3.0, "a map mark at {ratio:.2}:1 is too faint");
            }
        }
    }

    #[test]
    fn no_color_wins_over_every_other_signal() {
        assert_eq!(
            resolve(
                Some("1"),
                Some("light"),
                Some((255, 255, 255)),
                Some("15;7")
            ),
            Mode::Mono
        );
    }

    #[test]
    fn what_the_terminal_answers_outranks_what_the_environment_remembers() {
        // COLORFGBG says light; the terminal in front of the reader is dark.
        assert_eq!(
            resolve(None, None, Some((30, 30, 46)), Some("0;15")),
            Mode::Dark
        );
        assert_eq!(
            resolve(None, None, Some((250, 250, 250)), Some("15;0")),
            Mode::Light
        );
    }

    #[test]
    fn an_asked_for_theme_still_outranks_the_terminal() {
        assert_eq!(
            resolve(None, Some("light"), Some((0, 0, 0)), None),
            Mode::Light
        );
    }

    #[test]
    fn a_silent_terminal_leaves_the_environment_to_decide() {
        assert_eq!(resolve(None, None, None, Some("0;15")), Mode::Light);
        assert_eq!(resolve(None, None, None, None), Mode::Dark);
    }

    #[test]
    fn an_empty_no_color_is_not_a_signal() {
        assert_eq!(resolve(Some(""), Some("light"), None, None), Mode::Light);
    }

    #[test]
    fn an_unknown_theme_name_falls_through_instead_of_failing() {
        assert_eq!(
            resolve(None, Some("solarized"), None, Some("15;7")),
            Mode::Light
        );
        assert_eq!(resolve(None, Some("solarized"), None, None), Mode::Dark);
    }

    #[test]
    fn colorfgbg_reads_its_last_field_as_the_background() {
        assert_eq!(resolve(None, None, None, Some("15;7")), Mode::Light);
        assert_eq!(resolve(None, None, None, Some("0;15")), Mode::Light);
        assert_eq!(resolve(None, None, None, Some("15;0")), Mode::Dark);
        assert_eq!(resolve(None, None, None, Some("15;default")), Mode::Dark);
        assert_eq!(resolve(None, None, None, Some("")), Mode::Dark);
    }

    #[test]
    fn nothing_configured_means_dark() {
        assert_eq!(resolve(None, None, None, None), Mode::Dark);
    }

    #[test]
    fn each_mode_carries_its_own_syntax_theme() {
        assert_eq!(
            Palette::for_mode(Mode::Dark).syntect_theme,
            "base16-ocean.dark"
        );
        assert_eq!(
            Palette::for_mode(Mode::Light).syntect_theme,
            "InspiredGitHub"
        );
        assert!(!Palette::for_mode(Mode::Light).monochrome);
        assert!(Palette::for_mode(Mode::Mono).monochrome);
    }

    #[test]
    fn colour_modes_paint_syntax_and_monochrome_keeps_only_its_attributes() {
        let span = crate::model::SyntaxSpan {
            text: "fn".into(),
            rgb: (200, 100, 50),
            bold: true,
            italic: false,
        };
        let dark = Palette::for_mode(Mode::Dark).syntax(&span);
        assert_eq!(dark.fg, Some(Color::Rgb(200, 100, 50)));
        assert!(dark.add_modifier.contains(Modifier::BOLD));

        let mono = Palette::for_mode(Mode::Mono).syntax(&span);
        assert_eq!(mono.fg, None);
        assert!(mono.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn monochrome_emphasis_uses_attributes_rather_than_colour() {
        let mono = Palette::for_mode(Mode::Mono);
        // Underline, not reverse: a reversed cursor inside a reversed
        // visual-line row would merge into it and vanish.
        assert!(
            mono.cursor(Color::Reset)
                .add_modifier
                .contains(Modifier::UNDERLINED)
        );
        assert!(
            mono.selected(Style::default())
                .add_modifier
                .contains(Modifier::REVERSED)
        );
        assert!(
            mono.chip(Color::Reset)
                .add_modifier
                .contains(Modifier::REVERSED)
        );
        assert!(mono.caret().add_modifier.contains(Modifier::REVERSED));
    }

    #[test]
    fn colour_modes_keep_painting_emphasis_with_colour() {
        let dark = Palette::for_mode(Mode::Dark);
        assert_eq!(dark.cursor(dark.bg).bg, Some(dark.text));
        assert_eq!(dark.selected(Style::default()).bg, Some(dark.select_bg));
        assert_eq!(dark.chip(dark.blue).fg, Some(dark.blue));
        assert_eq!(dark.caret().bg, Some(dark.text));
    }

    #[test]
    fn monochrome_defers_every_colour_to_the_terminal() {
        let palette = Palette::for_mode(Mode::Mono);
        assert_eq!(palette.bg, Color::Reset);
        assert_eq!(palette.text, Color::Reset);
        assert_eq!(palette.select_bg, Color::Reset);
    }

    #[test]
    fn tests_never_read_the_ambient_environment() {
        assert_eq!(theme().bg, Palette::for_mode(Mode::Dark).bg);
    }
}
