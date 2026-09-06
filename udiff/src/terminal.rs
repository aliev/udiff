use crate::app::{App, Command, EditorTarget, Effect};
#[cfg(feature = "watch")]
use crate::input::{WatchInputEvent, WatchSource};
use crate::theme::theme;
use anyhow::{Context, Result};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    style::Style,
    widgets::{Block, Paragraph},
};
use std::{
    env,
    io::{self, Write},
    path::Path,
    process::Command as ProcessCommand,
    time::Duration,
};

pub struct TerminalRuntime;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EffectOutcome {
    Continue,
    Quit,
    ResetWatch,
}

impl TerminalRuntime {
    pub fn run(app: App) -> Result<Effect> {
        Self::run_inner(
            Some(app),
            #[cfg(feature = "watch")]
            None,
        )
    }

    #[cfg(feature = "watch")]
    pub fn run_watching(source: WatchSource, initial: Option<App>) -> Result<Effect> {
        Self::run_inner(initial, Some(source))
    }

    fn run_inner(
        mut app: Option<App>,
        #[cfg(feature = "watch")] watch_source: Option<WatchSource>,
    ) -> Result<Effect> {
        enable_raw_mode().context("enable raw mode")?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        let result = (|| -> Result<Effect> {
            loop {
                #[cfg(feature = "watch")]
                if let Some(source) = &watch_source {
                    while let Ok(event) = source.try_recv() {
                        apply_watch_event(&mut app, event);
                    }
                }
                #[cfg(feature = "watch")]
                let watched = watch_source.as_ref().map(WatchSource::root);
                #[cfg(not(feature = "watch"))]
                let watched: Option<&Path> = None;
                terminal.draw(|frame| match &mut app {
                    Some(app) => app.draw(frame),
                    None => draw_waiting(frame, watched),
                })?;
                if event::poll(Duration::from_millis(100))? {
                    match event::read()? {
                        Event::Key(key) => {
                            let outcome = if let Some(app) = &mut app {
                                let effect = app.update(Command::Key(key));
                                #[cfg(feature = "watch")]
                                let watch_root = watch_source.as_ref().map(WatchSource::root);
                                #[cfg(not(feature = "watch"))]
                                let watch_root: Option<&Path> = None;
                                handle_effect(app, &mut terminal, effect, watch_root)?
                            } else if key.code == crossterm::event::KeyCode::Char('q') {
                                EffectOutcome::Quit
                            } else {
                                EffectOutcome::Continue
                            };
                            if apply_effect_outcome(&mut app, outcome) {
                                break Ok(Effect::Quit);
                            }
                        }
                        Event::Mouse(mouse) => {
                            if let Some(app) = &mut app {
                                app.update(Command::Mouse(mouse));
                            }
                        }
                        Event::Resize(_, _) => {}
                        _ => {}
                    }
                }
            }
        })();
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;
        result
    }
}

fn apply_effect_outcome(app: &mut Option<App>, outcome: EffectOutcome) -> bool {
    match outcome {
        EffectOutcome::Continue => false,
        EffectOutcome::Quit => true,
        EffectOutcome::ResetWatch => {
            *app = None;
            false
        }
    }
}

#[cfg(feature = "watch")]
fn apply_watch_event(app: &mut Option<App>, event: WatchInputEvent) {
    match event {
        WatchInputEvent::Batch { number, files } => match app {
            Some(app) => {
                app.update(Command::RevisionReceived { number, files });
            }
            None => *app = Some(App::new_watching(number, files)),
        },
        WatchInputEvent::Error(error) => {
            if let Some(app) = app {
                app.update(Command::WatchError(error));
            }
        }
        WatchInputEvent::Closed => {
            if let Some(app) = app {
                app.update(Command::WatchStopped);
            }
        }
    }
}

fn draw_waiting(frame: &mut Frame, watched: Option<&Path>) {
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(theme().bg)),
        area,
    );
    let panel = waiting_panel(area);
    let mut lines = vec![ratatui::text::Line::from(ratatui::text::Span::styled(
        "\u{3bc}diff",
        Style::default()
            .fg(theme().text)
            .add_modifier(ratatui::style::Modifier::BOLD),
    ))];
    if let Some(root) = watched {
        lines.push(ratatui::text::Line::from(format!(
            "watching {}",
            compact_path(root, panel.width as usize)
        )));
    }
    lines.push(ratatui::text::Line::from(
        "waiting for changes \u{b7} q quit",
    ));
    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .style(Style::default().fg(theme().muted).bg(theme().bg)),
        panel,
    );
}

/// Keeps the tail of a watched path, which is the part that identifies it.
fn compact_path(path: &Path, width: usize) -> String {
    let full = path.display().to_string();
    let budget = width.saturating_sub("watching ".len());
    if full.chars().count() <= budget || budget <= 1 {
        return full;
    }
    let tail = full
        .chars()
        .skip(full.chars().count() - (budget - 1))
        .collect::<String>();
    format!("\u{2026}{tail}")
}

fn waiting_panel(area: ratatui::layout::Rect) -> ratatui::layout::Rect {
    let row = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(3.min(area.height)),
            Constraint::Fill(1),
        ])
        .split(area)[1];
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(56.min(area.width)),
            Constraint::Fill(1),
        ])
        .split(row)[1]
}

fn handle_effect(
    app: &mut App,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    effect: Effect,
    watch_root: Option<&Path>,
) -> Result<EffectOutcome> {
    match effect {
        Effect::None => {}
        Effect::ResetWatch => return Ok(EffectOutcome::ResetWatch),
        Effect::Copy(text) => {
            write!(terminal.backend_mut(), "{}", osc52_sequence(&text))?;
            terminal.backend_mut().flush()?;
        }
        Effect::OpenEditor(mut target) => {
            if let Some(root) = watch_root
                && Path::new(&target.path).is_relative()
            {
                target.path = root.join(&target.path).to_string_lossy().into_owned();
            }
            let result = open_in_editor(terminal, &target);
            if let Err(error) = result {
                app.notice(format!("editor: {error:#}"));
            }
        }
        Effect::Quit => return Ok(EffectOutcome::Quit),
    }
    Ok(EffectOutcome::Continue)
}

fn osc52_sequence(text: &str) -> String {
    let encoded = BASE64.encode(text);
    format!("\x1b]52;c;{encoded}\x07")
}

fn open_in_editor(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    target: &EditorTarget,
) -> Result<()> {
    disable_raw_mode()?;
    let editor_result = (|| -> Result<()> {
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;
        let mut command = editor_command(target)?;
        let status = command.status().context("start $EDITOR")?;
        anyhow::ensure!(status.success(), "$EDITOR exited with {status}");
        Ok(())
    })();

    let restore_result = (|| -> Result<()> {
        enable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            EnterAlternateScreen,
            EnableMouseCapture
        )?;
        reset_after_resume(terminal)?;
        Ok(())
    })();
    restore_result?;
    editor_result
}

fn reset_after_resume<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
) -> std::result::Result<(), B::Error> {
    terminal.autoresize()?;
    terminal.clear()
}

fn editor_command(target: &EditorTarget) -> Result<ProcessCommand> {
    let editor = env::var("EDITOR").context("$EDITOR is not set")?;
    editor_command_from(&editor, target)
}

fn editor_command_from(editor: &str, target: &EditorTarget) -> Result<ProcessCommand> {
    let mut parts = editor.split_whitespace();
    let program = parts.next().context("$EDITOR is empty")?;
    let mut command = ProcessCommand::new(program);
    command.args(parts).arg(&target.path);
    Ok(command)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, widgets::Paragraph};

    #[test]
    fn editor_command_preserves_configured_arguments_and_file_path() {
        let target = EditorTarget {
            path: "src/main file.rs".into(),
        };
        let command = editor_command_from("code --wait", &target).unwrap();
        assert_eq!(command.get_program(), "code");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            ["--wait", "src/main file.rs"]
        );
    }

    #[test]
    fn osc52_sequence_encodes_clipboard_text() {
        assert_eq!(
            osc52_sequence("review\ncomment"),
            "\x1b]52;c;cmV2aWV3CmNvbW1lbnQ=\x07"
        );
    }

    #[cfg(feature = "watch")]
    #[test]
    fn watch_mode_has_a_waiting_screen_before_the_first_batch() {
        let mut terminal = Terminal::new(TestBackend::new(80, 10)).unwrap();

        terminal.draw(|frame| draw_waiting(frame, None)).unwrap();

        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains("μdiff"));
        assert!(rendered.contains("waiting for changes"));
        assert!(rendered.contains("q quit"));
    }

    #[cfg(feature = "watch")]
    #[test]
    fn resetting_watch_drops_batches_and_accepts_the_next_one() {
        let files =
            crate::model::parse_unified_diff("--- old.rs\n+++ old.rs\n@@ -1 +1 @@\n-old\n+new\n");
        let mut app = Some(App::new_watching(1, files));

        assert!(!apply_effect_outcome(&mut app, EffectOutcome::ResetWatch));
        assert!(app.is_none());

        let files = crate::model::parse_unified_diff(
            "--- fresh.rs\n+++ fresh.rs\n@@ -1 +1 @@\n-old\n+new\n",
        );
        apply_watch_event(&mut app, WatchInputEvent::Batch { number: 2, files });
        assert!(app.is_some());
    }

    #[test]
    fn resume_forces_a_full_redraw_on_a_fresh_alternate_screen() {
        let mut terminal = Terminal::new(TestBackend::new(4, 1)).unwrap();
        terminal
            .draw(|frame| frame.render_widget(Paragraph::new("diff"), frame.area()))
            .unwrap();

        ratatui::backend::Backend::clear(terminal.backend_mut()).unwrap();
        terminal
            .draw(|frame| frame.render_widget(Paragraph::new("diff"), frame.area()))
            .unwrap();
        assert_eq!(terminal.backend().buffer().content()[0].symbol(), " ");

        reset_after_resume(&mut terminal).unwrap();
        terminal
            .draw(|frame| frame.render_widget(Paragraph::new("diff"), frame.area()))
            .unwrap();
        assert_eq!(terminal.backend().buffer().content()[0].symbol(), "d");
    }
}
