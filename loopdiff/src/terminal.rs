use crate::app::{App, BG, Command, EditorTarget, Effect, MUTED, TEXT};
use crate::input::{WatchInputEvent, WatchSource};
use anyhow::{Context, Result};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use chrono::{DateTime, Utc};
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

#[derive(Clone, Debug, Eq, PartialEq)]
struct HumanEdit {
    started_at: DateTime<Utc>,
    finished_at: DateTime<Utc>,
}

impl TerminalRuntime {
    pub fn run(app: App) -> Result<Effect> {
        Self::run_inner(Some(app), None)
    }

    pub fn run_watching(source: WatchSource, initial: Option<App>) -> Result<Effect> {
        Self::run_inner(initial, Some(source))
    }

    fn run_inner(mut app: Option<App>, watch_source: Option<WatchSource>) -> Result<Effect> {
        let mut human_edits = Vec::new();
        let human_name = git_user_name();
        enable_raw_mode().context("enable raw mode")?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        let result = (|| -> Result<Effect> {
            loop {
                if let Some(source) = &watch_source {
                    while let Ok(event) = source.try_recv() {
                        apply_watch_event(&mut app, event, &mut human_edits, &human_name);
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
                                handle_effect(
                                    app,
                                    &mut terminal,
                                    effect,
                                    watch_source.as_ref().map(WatchSource::root),
                                    &mut human_edits,
                                )?
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

fn apply_watch_event(
    app: &mut Option<App>,
    event: WatchInputEvent,
    human_edits: &mut Vec<HumanEdit>,
    human_name: &str,
) {
    match event {
        WatchInputEvent::Batch {
            number,
            started_at,
            finished_at,
            files,
        } => {
            let origin = revision_origin(human_edits, started_at, finished_at, human_name);
            match app {
                Some(app) => {
                    app.update(Command::BatchReceived {
                        number,
                        files,
                        origin,
                    });
                }
                None => *app = Some(App::new_watching(number, files)),
            }
        }
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

fn revision_origin(
    human_edits: &mut Vec<HumanEdit>,
    started_at: DateTime<Utc>,
    finished_at: DateTime<Utc>,
    human_name: &str,
) -> crate::app::RevisionOrigin {
    let human = human_edits
        .iter()
        .any(|edit| edit.started_at <= finished_at && started_at <= edit.finished_at);
    human_edits.retain(|edit| edit.finished_at >= started_at);
    if human {
        crate::app::RevisionOrigin::Human(human_name.to_owned())
    } else {
        crate::app::RevisionOrigin::Observed
    }
}

fn git_user_name() -> String {
    let configured = ProcessCommand::new("git")
        .args(["config", "--get", "user.name"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok());
    normalize_user_name(configured)
}

fn normalize_user_name(configured: Option<String>) -> String {
    configured
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "human".into())
}

fn draw_waiting(frame: &mut Frame) {
    let area = frame.area();
    frame.render_widget(Block::default().style(Style::default().bg(BG)), area);
    frame.render_widget(
        Paragraph::new(vec![
            ratatui::text::Line::from(ratatui::text::Span::styled(
                "loopdiff",
                Style::default()
                    .fg(TEXT)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            )),
            ratatui::text::Line::from("waiting for changes · q quit"),
        ])
        .alignment(Alignment::Center)
        .style(Style::default().fg(MUTED).bg(BG)),
        waiting_panel(area),
    );
}

fn waiting_panel(area: ratatui::layout::Rect) -> ratatui::layout::Rect {
    let row = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(2.min(area.height)),
            Constraint::Fill(1),
        ])
        .split(area)[1];
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(40.min(area.width)),
            Constraint::Fill(1),
        ])
        .split(row)[1]
}

fn handle_effect(
    app: &mut App,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    effect: Effect,
    watch_root: Option<&Path>,
    human_edits: &mut Vec<HumanEdit>,
) -> Result<EffectOutcome> {
    match effect {
        Effect::None => {}
        Effect::ResetWatch => return Ok(EffectOutcome::ResetWatch),
        Effect::Copy(text) => {
            write!(terminal.backend_mut(), "{}", osc52_sequence(&text))?;
            terminal.backend_mut().flush()?;
        }
        Effect::RequestFileView(file) => {
            app.update(Command::FileViewLoaded { file, lines: None });
        }
        Effect::OpenEditor(mut target) => {
            if let Some(root) = watch_root
                && Path::new(&target.path).is_relative()
            {
                target.path = root.join(&target.path).to_string_lossy().into_owned();
            }
            let started_at = Utc::now();
            let result = open_in_editor(terminal, &target);
            let finished_at = Utc::now();
            if target.capture_changes && result.is_ok() {
                human_edits.push(HumanEdit {
                    started_at,
                    finished_at,
                });
            }
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
    let configured_arguments = parts.collect::<Vec<_>>();
    let editor_name = std::path::Path::new(program)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(program);
    let mut command = ProcessCommand::new(program);
    command.args(&configured_arguments);
    match (editor_name, target.line) {
        ("hx" | "helix", Some(line)) => {
            command.arg(source_location(target, line));
        }
        ("vim" | "nvim" | "vi", Some(line)) => {
            command
                .arg(format!(
                    "+call cursor({line},{})",
                    target.column.unwrap_or(1)
                ))
                .arg(&target.path);
        }
        ("code" | "code-insiders" | "codium", Some(line)) => {
            if !configured_arguments.contains(&"--wait") {
                command.arg("--wait");
            }
            command.arg("--goto").arg(source_location(target, line));
        }
        ("zed", Some(line)) => {
            if !configured_arguments.contains(&"--wait") {
                command.arg("--wait");
            }
            command.arg(source_location(target, line));
        }
        _ => {
            command.arg(&target.path);
        }
    }
    Ok(command)
}

fn source_location(target: &EditorTarget, line: u32) -> String {
    format!("{}:{line}:{}", target.path, target.column.unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, widgets::Paragraph};

    #[test]
    fn editor_command_preserves_configured_arguments_and_file_path() {
        let target = EditorTarget {
            path: "src/main file.rs".into(),
            line: None,
            column: None,
            capture_changes: false,
        };
        let command = editor_command_from("code --wait", &target).unwrap();
        assert_eq!(command.get_program(), "code");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            ["--wait", "src/main file.rs"]
        );
    }

    #[test]
    fn editor_commands_use_native_source_locations() {
        let target = EditorTarget {
            path: "src/main file.rs".into(),
            line: Some(42),
            column: Some(9),
            capture_changes: true,
        };

        let helix = editor_command_from("hx", &target).unwrap();
        assert_eq!(
            helix.get_args().collect::<Vec<_>>(),
            ["src/main file.rs:42:9"]
        );

        let vscode = editor_command_from("code", &target).unwrap();
        assert_eq!(
            vscode.get_args().collect::<Vec<_>>(),
            ["--wait", "--goto", "src/main file.rs:42:9"]
        );

        let neovim = editor_command_from("nvim", &target).unwrap();
        assert_eq!(
            neovim.get_args().collect::<Vec<_>>(),
            ["+call cursor(42,9)", "src/main file.rs"]
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
        assert!(rendered.contains("waiting for changes"));
        assert!(rendered.contains("q quit"));
    }

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
        let now = Utc::now();
        apply_watch_event(
            &mut app,
            WatchInputEvent::Batch {
                number: 2,
                started_at: now,
                finished_at: now,
                files,
            },
            &mut Vec::new(),
            "human",
        );
        assert!(app.is_some());
    }

    #[test]
    fn revision_origin_uses_editor_session_overlap() {
        use chrono::TimeDelta;

        let editor_start = Utc::now();
        let editor_end = editor_start + TimeDelta::seconds(10);
        let mut edits = vec![HumanEdit {
            started_at: editor_start,
            finished_at: editor_end,
        }];

        assert_eq!(
            revision_origin(
                &mut edits,
                editor_start + TimeDelta::seconds(2),
                editor_start + TimeDelta::seconds(4),
                "Ada",
            ),
            crate::app::RevisionOrigin::Human("Ada".into())
        );
        assert_eq!(
            revision_origin(
                &mut edits,
                editor_end + TimeDelta::seconds(1),
                editor_end + TimeDelta::seconds(3),
                "Ada",
            ),
            crate::app::RevisionOrigin::Observed
        );
        assert!(edits.is_empty());
    }

    #[test]
    fn git_user_name_falls_back_for_missing_or_empty_values() {
        assert_eq!(
            normalize_user_name(Some("  Ada Lovelace\n".into())),
            "Ada Lovelace"
        );
        assert_eq!(normalize_user_name(Some("  ".into())), "human");
        assert_eq!(normalize_user_name(None), "human");
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
