# Theming: NO_COLOR and light terminals

μdiff hardcodes one dark palette. On a light terminal the interface is
unreadable, and `NO_COLOR` has no effect. This design gives μdiff three
appearance modes — dark, light, and monochrome — chosen from the environment.

## Scope

In scope: a palette selected at startup, a light palette, a monochrome mode
that honours `NO_COLOR`, and the syntax-highlighting theme that pairs with
each.

Out of scope: user-defined palettes, a config file, a `--theme` flag, and
querying the terminal over OSC 11. Each is a separate decision that this
design does not foreclose.

## Mode selection

`Palette::resolve` takes the three variables as plain arguments and returns a
mode; `Palette::from_environment` only reads them and delegates. The rules, in
order, with the first match winning:

1. `NO_COLOR` set to any non-empty value → monochrome. This follows
   <https://no-color.org>: colour is suppressed, other attributes are not.
2. `UDIFF_THEME` set to `dark`, `light`, or `mono` → that mode. Any other
   value is ignored rather than fatal; a typo in a shell profile must not stop
   a review.
3. `COLORFGBG` → its last `;`-separated field is the background colour index.
   `7` and `15` mean light; every other parsable value means dark.

If nothing matches, the mode is dark.

`COLORFGBG` is set by urxvt, konsole, and several others, and unset by iTerm2,
Terminal.app, Alacritty, and kitty. Users on those terminals set `UDIFF_THEME`.
This is the same arrangement delta uses, and it costs no dependency and no
startup round-trip to the terminal.

## Where the palette lives

A new `udiff/src/theme.rs` owns a `Palette` value and a process-wide
`OnceLock<Palette>` reached through `theme()`. The fourteen colour constants in
`udiff/src/app.rs` are deleted and their ~226 call sites become field reads:
`BG` becomes `theme().bg`. The non-colour constants beside them — `TAB_WIDTH`,
`PREVIOUS_BATCH_KEY`, `NEXT_BATCH_KEY` — stay where they are.

This mirrors `highlight.rs`, which already holds its syntect theme in a
`OnceLock`, so no new concept enters the codebase. It is a value, not a service
container, so it does not contradict `ARCHITECTURE.md`.

The alternative — threading `&Palette` through every `draw` — was rejected. It
would add a parameter to every widget and to `crop_spans`,
`inline_comment_lines`, `wrap_code_line`, `apply_block_cursor`, and
`styled_syntax_spans`, in exchange for flexibility that nothing needs: the
palette never changes while μdiff runs.

Under `cfg(test)` `theme()` resolves to the dark palette without reading the
environment, so the suite does not change behaviour when a developer exports
`UDIFF_THEME` or `NO_COLOR`.

## What monochrome means

Colour alone carries meaning at eight call sites. Setting every colour to
`Color::Reset` would erase them — most visibly the cursor, which is drawn today
by swapping foreground and background (`view_helpers.rs:58`), and would become
invisible when both are `Reset`.

So `Palette` exposes, besides its colour fields, four style constructors that
each mode answers in its own way:

| Constructor | Colour modes | Monochrome |
|---|---|---|
| `cursor()` | foreground and background swapped | `UNDERLINED` and bold |
| `selected(base)` | `base.bg(select_bg)` | `base` plus `REVERSED` |
| `chip(fg)` | `fg` on `select_bg`, bold | `REVERSED` and bold |
| `caret()` | background `text` | `REVERSED` |

Visual-line selection is the one case that does not fit, because
`diff_view::diff_line` carries the selection as a bare `Color` used for both
the row background and its trailing filler. Rather than turn that colour into a
`Style` through three helper signatures, monochrome applies `REVERSED` to the
row's spans as a final pass in `diff_line`. The colour plumbing is untouched.

This is why the monochrome cursor underlines rather than reverses. `Modifier`
is a bitflag, so a reversed cursor inside a reversed row would merge into the
row and disappear; an underline stays visible against both a plain row and a
reversed one.

Add and remove lines keep their `+` and `-` gutter markers, comment cards keep
their `┃` bar and `Comment #N` label, and the review meter already uses two
different glyphs, so those distinctions survive without colour.

## Syntax highlighting

`highlight.rs` writes RGB values straight into `SyntaxSpan.rgb` at parse time,
which is a second source of colour independent of the palette. The two modes
touch it differently:

- **Light and dark** choose the syntect theme, because the RGB values are baked
  in during `parse_unified_diff`. The choice moves from `highlight.rs:63` to
  `theme.rs`: dark keeps `base16-ocean.dark`, light takes `InspiredGitHub`.
  `main::run` therefore calls `theme::init()` as its first statement, before
  any diff is parsed. Lazy initialisation would land on the same answer, but an
  explicit call keeps the ordering visible rather than incidental.
- **Monochrome** is a render-time concern only. Highlighting still runs, and
  the two places that consume `SyntaxSpan` — `diff_view::diff_line` and
  `render::styled_syntax_spans` — ask the palette for the style. In monochrome
  it drops the foreground and keeps bold and italic.

Keeping monochrome out of parse time means `model.rs` never depends on
initialisation order, and its highlighting tests, which assert only that two
scopes differ, hold under any theme.

## Palette values

Dark keeps today's values. Light uses GitHub Primer, matching the quiet
GitHub-like interface μdiff already aims at:

| Field | Dark | Light |
|---|---|---|
| `bg` | `15,18,25` | `255,255,255` |
| `surface` | `21,25,35` | `237,241,245` |
| `border` | `45,51,66` | `208,215,222` |
| `text` | `220,224,232` | `31,35,40` |
| `muted` | `122,132,153` | `101,109,118` |
| `blue` | `122,162,247` | `9,105,218` |
| `green` | `158,206,106` | `26,127,55` |
| `green_bg` | `24,45,35` | `218,251,225` |
| `red` | `247,118,142` | `207,34,46` |
| `red_bg` | `54,31,40` | `255,235,233` |
| `hunk_bg` | `28,38,58` | `221,244,255` |
| `comment` | `224,175,104` | `154,103,0` |
| `comment_bg` | `47,39,28` | `255,248,197` |
| `select_bg` | `38,49,70` | `221,232,244` |

Monochrome sets every field to `Color::Reset`.

## Testing

Selection is a pure function and is tested directly: `NO_COLOR` wins over
`UDIFF_THEME`, an unknown `UDIFF_THEME` falls through rather than failing,
`COLORFGBG` of `15;7` reads light and `15;0` reads dark, a malformed
`COLORFGBG` falls back to dark.

Rendering tests keep asserting against the dark palette, which `cfg(test)`
guarantees. Two new render tests cover what colour cannot express: in
monochrome the cursor cell carries `REVERSED`, and a highlighted line's spans
carry no foreground while keeping their bold and italic.

## Documentation

`README.md` gains a short section on `NO_COLOR`, `UDIFF_THEME`, and
`COLORFGBG`. `ARCHITECTURE.md` gains `theme.rs` in its reading order and a row
in "Where a change belongs". `CHANGELOG.md` records the modes under the
unreleased 0.1.0 entry.
