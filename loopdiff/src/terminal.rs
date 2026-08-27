use crate::app::{App, Command, Effect};
use crate::input::{WatchInputEvent, WatchSource};
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
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph},
};
use std::{
    env,
    io::{self, Write},
    process::Command as ProcessCommand,
    time::Duration,
};

pub struct TerminalRuntime;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EffectOutcome {
    Continue,
    Quit,
}

impl TerminalRuntime {
    pub fn run(app: App) -> Result<Effect> {
        Self::run_inner(Some(app), None)
    }

    pub fn run_watching(source: WatchSource) -> Result<Effect> {
        Self::run_inner(None, Some(source))
    }

    fn run_inner(mut app: Option<App>, watch_source: Option<WatchSource>) -> Result<Effect> {
        enable_raw_mode().context("enable raw mode")?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        let result = (|| -> Result<Effect> {
            loop {
                if let Some(source) = &watch_source {
                    while let Ok(event) = source.try_recv() {
                        apply_watch_event(&mut app, event);
                    }
                }
                terminal.draw(|frame| match &mut app {
                    Some(app) => app.draw(frame),
                    None => draw_waiting(frame),
                })?;
                if event::poll(Duration::from_millis(100))? {
                    match event::read()? {
                        Event::Key(key) => {
                            let outcome = if let Some(app) = &mut app {
                                let effect = app.update(Command::Key(key));
                                handle_effect(app, &mut terminal, effect)?
                            } else if key.code == crossterm::event::KeyCode::Char('q') {
                                EffectOutcome::Quit
                            } else {
                                EffectOutcome::Continue
                            };
                            if outcome == EffectOutcome::Quit {
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

fn apply_watch_event(app: &mut Option<App>, event: WatchInputEvent) {
    match event {
        WatchInputEvent::Batch { number, files } => match app {
            Some(app) => {
                app.update(Command::BatchReceived { number, files });
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

fn draw_waiting(frame: &mut Frame) {
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(Color::Rgb(13, 17, 23))),
        area,
    );
    frame.render_widget(
        Paragraph::new("Waiting for file changes… · q quit")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Rgb(139, 148, 158)))
            .block(Block::default().borders(Borders::TOP | Borders::BOTTOM)),
        waiting_panel(area),
    );
}

fn waiting_panel(area: ratatui::layout::Rect) -> ratatui::layout::Rect {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(45),
            Constraint::Length(3),
            Constraint::Percentage(55),
        ])
        .split(area)[1]
}

fn handle_effect(
    app: &mut App,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    effect: Effect,
) -> Result<EffectOutcome> {
    match effect {
        Effect::None => {}
        Effect::Copy(text) => {
            write!(terminal.backend_mut(), "{}", osc52_sequence(&text))?;
            terminal.backend_mut().flush()?;
        }
        Effect::RequestFileView(file) => {
            app.update(Command::FileViewLoaded { file, lines: None });
        }
        Effect::OpenFile(path) => {
            if let Err(error) = open_in_editor(terminal, &path) {
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

fn open_in_editor(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, path: &str) -> Result<()> {
    disable_raw_mode()?;
    let editor_result = (|| -> Result<()> {
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;
        let mut command = editor_command(path)?;
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

fn editor_command(path: &str) -> Result<ProcessCommand> {
    let editor = env::var("EDITOR").context("$EDITOR is not set")?;
    editor_command_from(&editor, path)
}

fn editor_command_from(editor: &str, path: &str) -> Result<ProcessCommand> {
    let mut parts = editor.split_whitespace();
    let program = parts.next().context("$EDITOR is empty")?;
    let mut command = ProcessCommand::new(program);
    command.args(parts).arg(path);
    Ok(command)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, widgets::Paragraph};

    #[test]
    fn editor_command_preserves_configured_arguments_and_file_path() {
        let command = editor_command_from("code --wait", "src/main file.rs").unwrap();
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

    #[test]
    fn watch_mode_has_a_waiting_screen_before_the_first_batch() {
        let mut terminal = Terminal::new(TestBackend::new(80, 10)).unwrap();

        terminal.draw(draw_waiting).unwrap();

        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains("Waiting for file changes"));
        assert!(rendered.contains("q quit"));
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
