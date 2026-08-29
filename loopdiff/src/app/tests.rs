use super::file_tree::Target as SideTarget;
use super::render::{crop_spans, file_status_spans, inline_comment_lines};
use super::view_helpers::*;
use super::*;
use crate::comment::Comment;
use crate::model::{FileStatus, parse_unified_diff};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Terminal,
    backend::TestBackend,
    style::{Color, Modifier, Style},
    text::Span,
};

#[test]
fn update_exposes_commands_and_effects_at_the_app_boundary() {
    let diff = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n+hello\n";
    let mut app = App::new(parse_unified_diff(diff), Vec::new());

    assert!(matches!(
        app.update(Command::Key(KeyEvent::new(
            KeyCode::Char('q'),
            KeyModifiers::NONE
        ))),
        Effect::Quit
    ));
}

#[test]
fn shift_r_resets_only_watch_mode() {
    let files = || parse_unified_diff("--- a.rs\n+++ a.rs\n@@ -1 +1 @@\n-old\n+new\n");
    let key = KeyEvent::new(KeyCode::Char('R'), KeyModifiers::SHIFT);

    let mut watching = App::new_watching(1, files());
    assert_eq!(watching.update(Command::Key(key)), Effect::ResetWatch);

    let mut static_diff = App::new(files(), Vec::new());
    assert_eq!(static_diff.update(Command::Key(key)), Effect::None);
}

#[test]
fn watched_batches_follow_latest_and_keep_independent_state() {
    let first = parse_unified_diff("--- first.rs\n+++ first.rs\n@@ -1 +1 @@\n-old\n+first\n");
    let second = parse_unified_diff("--- second.rs\n+++ second.rs\n@@ -1 +1 @@\n-old\n+second\n");
    let mut app = App::new_watching(10, first);
    app.session.reviewed_files.insert(0);

    app.update(Command::BatchReceived {
        number: 11,
        files: second,
        origin: RevisionOrigin::Human("Ada".into()),
    });

    assert_eq!(app.batch_number, 11);
    assert_eq!(app.revision_origin, RevisionOrigin::Human("Ada".into()));
    assert_eq!(app.current().path, "second.rs");
    assert!(app.session.reviewed_files.is_empty());

    app.key(KeyEvent::new(
        KeyCode::Char(PREVIOUS_BATCH_KEY),
        KeyModifiers::NONE,
    ));
    assert_eq!(app.batch_number, 10);
    assert_eq!(app.revision_origin, RevisionOrigin::Observed);
    assert_eq!(app.current().path, "first.rs");
    assert!(app.session.reviewed_files.contains(&0));

    app.key(KeyEvent::new(
        KeyCode::Char(NEXT_BATCH_KEY),
        KeyModifiers::NONE,
    ));
    assert_eq!(app.batch_number, 11);
    assert_eq!(app.revision_origin, RevisionOrigin::Human("Ada".into()));
}

#[test]
fn piped_context_precedes_numbered_watch_revisions() {
    let context =
        parse_unified_diff("--- context.rs\n+++ context.rs\n@@ -1 +1 @@\n-old\n+context\n");
    let live = parse_unified_diff("--- live.rs\n+++ live.rs\n@@ -1 +1 @@\n-old\n+live\n");
    let mut app = App::new_watching_context(context);

    assert_eq!(app.batch_number, 0);
    assert_eq!(app.revision_origin, RevisionOrigin::Context);
    app.update(Command::BatchReceived {
        number: 1,
        files: live,
        origin: RevisionOrigin::Observed,
    });
    assert_eq!(app.batch_number, 1);
    assert_eq!(app.current().path, "live.rs");

    app.key(KeyEvent::new(
        KeyCode::Char(PREVIOUS_BATCH_KEY),
        KeyModifiers::NONE,
    ));
    assert_eq!(app.batch_number, 0);
    assert_eq!(app.revision_origin, RevisionOrigin::Context);
    assert_eq!(app.current().path, "context.rs");
}

#[test]
fn shift_s_copies_only_the_current_human_revision() {
    let first = parse_unified_diff("--- first.rs\n+++ first.rs\n@@ -1 +1 @@\n-old\n+first\n");
    let second = parse_unified_diff("--- second.rs\n+++ second.rs\n@@ -1 +1 @@\n-old\n+second\n");
    let mut app = App::new_watching(10, first);
    let key = KeyEvent::new(KeyCode::Char('S'), KeyModifiers::SHIFT);

    assert_eq!(app.update(Command::Key(key)), Effect::None);
    app.update(Command::BatchReceived {
        number: 11,
        files: second,
        origin: RevisionOrigin::Human("Ada".into()),
    });

    let Effect::Copy(handoff) = app.update(Command::Key(key)) else {
        panic!("Shift+S should copy a human revision");
    };
    assert!(handoff.contains("Ada revision #11"));
    assert!(handoff.contains("- second.rs"));
    assert!(handoff.contains("-old\n+second"));
    assert!(handoff.contains("pair-programming input from Ada"));
}

#[test]
fn incoming_batch_does_not_interrupt_reviewing_history() {
    let files = || parse_unified_diff("--- file.rs\n+++ file.rs\n@@ -1 +1 @@\n-old\n+new\n");
    let mut app = App::new_watching(1, files());
    app.update(Command::BatchReceived {
        number: 2,
        files: files(),
        origin: RevisionOrigin::Observed,
    });
    app.key(KeyEvent::new(
        KeyCode::Char(PREVIOUS_BATCH_KEY),
        KeyModifiers::NONE,
    ));

    app.update(Command::BatchReceived {
        number: 3,
        files: files(),
        origin: RevisionOrigin::Observed,
    });

    assert_eq!(app.batch_number, 1);
    assert_eq!(app.active_batch, 0);
    assert_eq!(app.batch_states.len(), 3);
}

#[test]
fn e_requests_opening_the_current_file_in_editor() {
    let diff = "diff --git a/src/a.rs b/src/a.rs\n--- a/src/a.rs\n+++ b/src/a.rs\n@@ -1 +1 @@\n-old\n+new\n";
    let mut app = App::new(parse_unified_diff(diff), Vec::new());

    assert_eq!(
        app.update(Command::Key(KeyEvent::new(
            KeyCode::Char('e'),
            KeyModifiers::NONE,
        ))),
        Effect::OpenEditor(EditorTarget {
            path: "src/a.rs".into(),
            line: None,
            column: None,
            capture_changes: false,
        })
    );
}

#[test]
fn s_opens_the_current_code_location_for_editing() {
    let diff = "diff --git a/src/a.rs b/src/a.rs\n--- a/src/a.rs\n+++ b/src/a.rs\n@@ -9 +9 @@\n-old\n+    new\n";
    let mut app = App::new(parse_unified_diff(diff), Vec::new());
    app.diff_pane.cursor = 2;

    assert_eq!(
        app.update(Command::Key(KeyEvent::new(
            KeyCode::Char('s'),
            KeyModifiers::NONE,
        ))),
        Effect::OpenEditor(EditorTarget {
            path: "src/a.rs".into(),
            line: Some(9),
            column: Some(5),
            capture_changes: true,
        })
    );
}

#[test]
fn renders_complete_layout() {
    let diff = "diff --git a/src/main.rs b/src/main.rs\n--- a/src/main.rs\n+++ b/src/main.rs\n@@ -1 +1 @@\n-fn old() {}\n+fn new() {}\n";
    let mut app = App::new(parse_unified_diff(diff), Vec::new());
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal.draw(|frame| app.draw(frame)).unwrap();
    assert!(
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .any(|cell| cell.symbol() == "l")
    );
    // Changed-line background reaches the right edge of the diff viewport.
    assert_eq!(
        terminal.backend().buffer().cell((99, 3)).unwrap().bg,
        RED_BG
    );
    assert_eq!(
        terminal.backend().buffer().cell((99, 4)).unwrap().bg,
        GREEN_BG
    );
}

#[test]
fn long_code_lines_wrap_in_diff_and_file_views() {
    let code = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789WRAPPED";
    let diff = format!(
        "diff --git a/main.go b/main.go\n--- a/main.go\n+++ b/main.go\n@@ -0,0 +1 @@\n+{code}\n"
    );
    let mut app = App::new(parse_unified_diff(&diff), Vec::new());
    app.set_file_views(vec![Some(crate::model::file_view_lines(
        "main.go",
        &format!("{code}\n"),
    ))]);
    let mut terminal = Terminal::new(TestBackend::new(100, 20)).unwrap();
    let row_text = |terminal: &Terminal<TestBackend>, row| {
        (36..100)
            .map(|column| {
                terminal
                    .backend()
                    .buffer()
                    .cell((column, row))
                    .unwrap()
                    .symbol()
            })
            .collect::<String>()
    };

    terminal.draw(|frame| app.draw(frame)).unwrap();
    assert!(row_text(&terminal, 4).contains("WRAPPED"));

    app.key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));
    terminal.draw(|frame| app.draw(frame)).unwrap();
    assert!(row_text(&terminal, 3).contains("WRAPPED"));
}

#[test]
fn last_wrapped_line_remains_visible_at_the_end_of_diff_and_file_views() {
    let source = (1..=70)
        .map(|number| {
            if number == 70 {
                format!("LAST_LINE_{}", "x".repeat(64))
            } else {
                format!("line_{number:02}_{}", "x".repeat(64))
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let added = source
        .lines()
        .map(|line| format!("+{line}\n"))
        .collect::<String>();
    let diff = format!(
        "diff --git a/tasks.md b/tasks.md\n--- /dev/null\n+++ b/tasks.md\n@@ -0,0 +1,70 @@\n{added}"
    );
    let mut app = App::new(parse_unified_diff(&diff), Vec::new());
    app.set_file_views(vec![Some(crate::model::file_view_lines(
        "tasks.md", &source,
    ))]);
    let mut terminal = Terminal::new(TestBackend::new(100, 20)).unwrap();
    let last_line_is_visible = |terminal: &Terminal<TestBackend>| {
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>()
            .contains("LAST_LINE")
    };

    app.key(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::NONE));
    terminal.draw(|frame| app.draw(frame)).unwrap();
    assert!(last_line_is_visible(&terminal));

    app.key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));
    terminal.draw(|frame| app.draw(frame)).unwrap();
    assert!(last_line_is_visible(&terminal));
}

#[test]
fn o_toggles_full_file_and_preserves_diff_position() {
    let diff = "diff --git a/src/main.rs b/src/main.rs\n--- a/src/main.rs\n+++ b/src/main.rs\n@@ -8 +8 @@\n-old\n+new\n";
    let mut app = App::new(parse_unified_diff(diff), Vec::new());
    app.diff_pane.cursor = 2;
    app.set_file_views(vec![Some(crate::model::file_view_lines(
        "src/main.rs",
        "one\ntwo\nthree\nfour\nfive\nsix\nseven\nnew\nnine\n",
    ))]);

    app.key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));
    assert!(app.diff_pane.file_view);
    assert_eq!(app.diff_pane.cursor, 7);
    assert_eq!(
        app.diff_pane.active_lines(&app.session.files)[app.diff_pane.cursor].text,
        "new"
    );

    app.key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));
    assert!(!app.diff_pane.file_view);
    assert_eq!(app.diff_pane.cursor, 2);
}

#[test]
fn full_file_is_requested_lazily_and_cached() {
    let diff = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-old\n+new\n";
    let mut app = App::new(parse_unified_diff(diff), Vec::new());

    assert!(matches!(
        app.key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE)),
        Outcome::LoadFileView(0)
    ));
    assert!(!app.diff_pane.file_view);

    app.finish_file_view_load(
        0,
        Some(crate::model::file_view_lines("a.rs", "new\ncontext\n")),
    );
    assert!(app.diff_pane.file_view);

    app.key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));
    assert!(!app.diff_pane.file_view);
    assert!(matches!(
        app.key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE)),
        Outcome::Continue
    ));
    assert!(app.diff_pane.file_view);
}

#[test]
fn full_file_view_is_read_only_but_supports_visual_yank() {
    let diff = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-old\n+new\n";
    let mut app = App::new(parse_unified_diff(diff), Vec::new());
    app.set_file_views(vec![Some(crate::model::file_view_lines(
        "a.rs",
        "let first = 1;\nlet second = 2;\n",
    ))]);
    app.key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));
    app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(app.focus, Focus::Diff);
    assert!(app.comment_editor.anchor.is_none());

    app.key(KeyEvent::new(KeyCode::Char('V'), KeyModifiers::SHIFT));
    app.key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    let outcome = app.key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));
    assert!(
        matches!(outcome, Outcome::Yank(ref text) if text == "let first = 1;\nlet second = 2;")
    );
}

#[test]
fn scrolled_out_hunk_header_sticks_without_duplication() {
    let diff = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@ first\n one\n@@ -20 +20 @@ second\n two\n";
    let mut app = App::new(parse_unified_diff(diff), Vec::new());
    app.diff_pane.cursor = 3;
    app.diff_pane.scroll = 3;
    let mut terminal = Terminal::new(TestBackend::new(100, 20)).unwrap();

    terminal.draw(|frame| app.draw(frame)).unwrap();

    let screen = terminal
        .backend()
        .buffer()
        .content()
        .chunks(100)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>();
    assert!(screen[2].contains("@@ -20 +20 @@ second"));
    assert_eq!(
        screen
            .iter()
            .filter(|row| row.contains("@@ -20 +20 @@ second"))
            .count(),
        1
    );

    // Once the next hunk reaches the top, its real row replaces the old sticky row.
    app.diff_pane.scroll = 2;
    terminal.draw(|frame| app.draw(frame)).unwrap();
    let screen = terminal
        .backend()
        .buffer()
        .content()
        .chunks(100)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>();
    assert!(screen[2].contains("@@ -20 +20 @@ second"));
    assert!(!screen[2].contains("@@ -1 +1 @@ first"));
}

#[test]
fn tab_moves_focus_accent_between_panels() {
    let diff = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-old\n+new\n";
    let mut app = App::new(parse_unified_diff(diff), Vec::new());
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal.draw(|frame| app.draw(frame)).unwrap();
    assert_eq!(terminal.backend().buffer().cell((50, 1)).unwrap().fg, BLUE);
    assert_eq!(
        terminal.backend().buffer().cell((35, 4)).unwrap().fg,
        BORDER
    );

    app.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    terminal.draw(|frame| app.draw(frame)).unwrap();
    assert_eq!(
        terminal.backend().buffer().cell((50, 1)).unwrap().fg,
        BORDER
    );
    assert_eq!(terminal.backend().buffer().cell((35, 4)).unwrap().fg, BLUE);
}

#[test]
fn minus_toggles_between_explorer_and_diff() {
    let diff = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-old\n+new\n";
    let mut app = App::new(parse_unified_diff(diff), Vec::new());
    app.key(KeyEvent::new(KeyCode::Char('-'), KeyModifiers::NONE));
    assert_eq!(app.focus, Focus::Files);
    assert_eq!(app.file_tree.selection(), Some(SideTarget::File(0)));
    app.key(KeyEvent::new(KeyCode::Char('-'), KeyModifiers::NONE));
    assert_eq!(app.focus, Focus::Diff);
}

#[test]
fn question_mark_opens_modal_help_until_closed() {
    let diff = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-old\n+new\n";
    let mut app = App::new(parse_unified_diff(diff), Vec::new());
    app.key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
    assert!(app.help.is_open());
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal.draw(|frame| app.draw(frame)).unwrap();
    let rendered = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(rendered.contains("Help"));

    app.key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    assert!(app.help.is_open());
    app.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(!app.help.is_open());
}

#[test]
fn help_renders_shift_y_copy_command() {
    let diff = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-old\n+new\n";
    let mut app = App::new(parse_unified_diff(diff), Vec::new());
    app.help.open();
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();

    terminal.draw(|frame| app.draw(frame)).unwrap();

    let rendered = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(rendered.contains("Shift+Y"));
    assert!(rendered.contains("copy comments"));
}

#[test]
fn numbered_gg_jumps_to_exact_or_nearest_diff_line() {
    let diff = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -120,3 +120,3 @@\n one\n two\n three\n";
    let mut app = App::new(parse_unified_diff(diff), Vec::new());
    for character in "121gg".chars() {
        app.key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE));
    }
    assert_eq!(app.current().lines[app.diff_pane.cursor].new, Some(121));
    assert!(app.diff_pane.vim_command.is_empty());

    for character in "999gg".chars() {
        app.key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE));
    }
    assert_eq!(app.current().lines[app.diff_pane.cursor].new, Some(122));
}

#[test]
fn escape_cancels_search_and_restores_previous_state() {
    let diff = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-old\n+new\n";
    let mut app = App::new(parse_unified_diff(diff), Vec::new());
    app.file_tree.set_filter("previous");
    app.key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
    assert_eq!(app.focus, Focus::Filter);
    assert_eq!(app.file_tree.filter(), "previous");
    app.key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
    assert_eq!(app.file_tree.filter(), "previousa");
    app.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(app.focus, Focus::Diff);
    assert_eq!(app.file_tree.filter(), "previous");
}

#[test]
fn enter_accepts_search_in_file_explorer_and_empty_search_clears_it() {
    let diff = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-old\n+new\n";
    let template = parse_unified_diff(diff).remove(0);
    let mut second = template.clone();
    second.path = "second.rs".into();
    let mut app = App::new(vec![template, second], Vec::new());

    app.key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
    for character in "second".chars() {
        app.key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE));
    }
    app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(app.focus, Focus::Files);
    assert_eq!(app.diff_pane.file, 1);
    assert_eq!(app.file_tree.filter(), "second");

    app.key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
    assert_eq!(app.file_tree.filter(), "second");
    app.key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(app.focus, Focus::Files);
    assert!(app.file_tree.filter().is_empty());
}

#[test]
fn reverse_range_places_new_comment_at_visual_bottom() {
    let diff = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-old\n+new\n";
    let mut app = App::new(parse_unified_diff(diff), Vec::new());
    app.diff_pane.range_anchor = Some(2);
    app.diff_pane.cursor = 1;

    app.open_editor();
    assert_eq!(app.comment_editor.anchor, Some(2));
    app.comment_editor.text = "Looks good?".into();
    app.save_editor();

    assert_eq!(app.session.comments[0].anchor_new, Some(1));
    assert_eq!(app.session.comments[0].anchor_old, None);
}

#[test]
fn enter_inside_existing_range_starts_a_new_comment() {
    let diff =
        "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1,3 +1,3 @@\n one\n two\n three\n";
    let outer = Comment {
        id: "t-001".into(),
        path: "a.rs".into(),
        excerpt: " one\n two\n three".into(),
        old_start: Some(1),
        old_end: Some(3),
        new_start: Some(1),
        new_end: Some(3),
        anchor_old: Some(3),
        anchor_new: Some(3),
        text: "Outer".into(),
    };
    let mut app = App::new(parse_unified_diff(diff), vec![outer]);
    app.diff_pane.cursor = 2;

    app.open_editor();

    assert!(app.comment_editor.editing_key.is_none());
    assert!(app.comment_editor.text.is_empty());
    assert_eq!(app.comment_editor.anchor, Some(2));
}

#[test]
fn u_restores_the_last_deleted_comment() {
    let diff = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n line\n";
    let comment = Comment {
        id: "t-001".into(),
        path: "a.rs".into(),
        excerpt: " line".into(),
        old_start: Some(1),
        old_end: Some(1),
        new_start: Some(1),
        new_end: Some(1),
        anchor_old: Some(1),
        anchor_new: Some(1),
        text: "Why?".into(),
    };
    let mut app = App::new(parse_unified_diff(diff), vec![comment.clone()]);
    app.diff_pane.cursor = 1;

    app.key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));
    assert!(app.session.comments.is_empty());
    app.key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE));

    assert_eq!(app.session.comments, vec![comment]);
}

#[test]
fn shift_v_selects_lines_and_y_yanks_code_without_diff_prefixes() {
    let diff =
        "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1,2 +1,2 @@\n-old\n+new\n same\n";
    let mut app = App::new(parse_unified_diff(diff), Vec::new());
    app.diff_pane.cursor = 1;

    app.key(KeyEvent::new(KeyCode::Char('V'), KeyModifiers::SHIFT));
    app.key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    let outcome = app.key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));

    assert!(matches!(outcome, Outcome::Yank(ref code) if code == "old\nnew"));
    assert!(app.diff_pane.range_anchor.is_none());
}

#[test]
fn shift_y_copies_comments_as_plain_text() {
    let diff = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-old\n+new\n";
    let comment = |id: &str, text: &str| Comment {
        id: id.into(),
        path: "a.rs".into(),
        excerpt: "-old\n+new".into(),
        old_start: Some(1),
        old_end: Some(1),
        new_start: Some(1),
        new_end: Some(1),
        anchor_old: None,
        anchor_new: Some(1),
        text: text.into(),
    };
    let mut app = App::new(
        parse_unified_diff(diff),
        vec![
            comment("first", "Please simplify this."),
            comment("second", "Check this too."),
        ],
    );

    let outcome = app.key(KeyEvent::new(KeyCode::Char('Y'), KeyModifiers::SHIFT));

    let Outcome::Yank(text) = outcome else {
        panic!("Shift+Y should copy comments");
    };
    assert!(text.starts_with("1. a.rs (old lines 1; new lines 1)"));
    assert!(text.contains("Selected diff:\n-old\n+new\nComment: Please simplify this."));
    assert!(text.contains("2. a.rs"));
    assert!(!text.contains("format_version"));
}

#[test]
fn shift_y_copies_comments_from_all_watched_batches() {
    let files = || parse_unified_diff("--- a.rs\n+++ a.rs\n@@ -1 +1 @@\n-old\n+new\n");
    let comment = |id: &str, text: &str| Comment {
        id: id.into(),
        path: "a.rs".into(),
        excerpt: "-old\n+new".into(),
        old_start: Some(1),
        old_end: Some(1),
        new_start: Some(1),
        new_end: Some(1),
        anchor_old: None,
        anchor_new: Some(1),
        text: text.into(),
    };
    let mut app = App::new_watching(1, files());
    app.session
        .comments
        .push(comment("first", "First revision"));
    app.update(Command::BatchReceived {
        number: 2,
        files: files(),
        origin: RevisionOrigin::Observed,
    });
    app.session
        .comments
        .push(comment("second", "Second revision"));

    let outcome = app.key(KeyEvent::new(KeyCode::Char('Y'), KeyModifiers::SHIFT));

    let Outcome::Yank(text) = outcome else {
        panic!("Shift+Y should copy comments");
    };
    assert!(text.contains("1. a.rs"));
    assert!(text.contains("Comment: First revision"));
    assert!(text.contains("2. a.rs"));
    assert!(text.contains("Comment: Second revision"));
    assert!(text.find("First revision") < text.find("Second revision"));
}

#[test]
fn shift_y_skips_comments_from_reviewed_files_in_every_batch() {
    let files = || parse_unified_diff("--- a.rs\n+++ a.rs\n@@ -1 +1 @@\n-old\n+new\n");
    let comment = |id: &str, text: &str| Comment {
        id: id.into(),
        path: "a.rs".into(),
        excerpt: "-old\n+new".into(),
        old_start: Some(1),
        old_end: Some(1),
        new_start: Some(1),
        new_end: Some(1),
        anchor_old: None,
        anchor_new: Some(1),
        text: text.into(),
    };
    let mut app = App::new_watching(1, files());
    app.session
        .comments
        .push(comment("first", "Reviewed revision"));
    app.session.reviewed_files.insert(0);
    app.update(Command::BatchReceived {
        number: 2,
        files: files(),
        origin: RevisionOrigin::Observed,
    });
    app.session
        .comments
        .push(comment("second", "Pending revision"));

    let outcome = app.key(KeyEvent::new(KeyCode::Char('Y'), KeyModifiers::SHIFT));

    let Outcome::Yank(text) = outcome else {
        panic!("Shift+Y should copy comments from unreviewed files");
    };
    assert!(!text.contains("Reviewed revision"));
    assert!(text.contains("Pending revision"));
}

#[test]
fn v_selects_characters_for_yank_without_creating_comment_range() {
    let diff = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n+hello\n";
    let mut app = App::new(parse_unified_diff(diff), Vec::new());
    app.diff_pane.cursor = 1;

    app.key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
    app.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    app.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    let outcome = app.key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));

    assert!(matches!(outcome, Outcome::Yank(ref code) if code == "hel"));
    assert!(app.diff_pane.range_anchor.is_none());
}

#[test]
fn characterwise_visual_mode_renders_a_distinct_block_cursor() {
    let diff = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n+hello\n";
    let mut app = App::new(parse_unified_diff(diff), Vec::new());
    app.diff_pane.cursor = 1;
    app.key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
    app.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    app.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    let line = app.current().lines[1].clone();

    let rendered =
        app.diff_pane
            .line_for_test(&app.session, &app.comment_editor, app.focus, &line, 1, 40);
    let cursor = rendered
        .spans
        .iter()
        .find(|span| span.content == "l" && span.style.fg == Some(BG))
        .unwrap();

    assert_eq!(cursor.style.bg, Some(TEXT));
    assert!(cursor.style.add_modifier.contains(Modifier::BOLD));
}

#[test]
fn characterwise_selection_preserves_syntax_foreground() {
    let syntax_color = Color::Rgb(214, 93, 14);
    let mut spans = vec![
        Span::raw("prefix"),
        Span::styled("token", Style::default().fg(syntax_color).bg(GREEN_BG)),
    ];

    apply_character_selection(&mut spans, 1, 0, 3, Some(3));

    assert_eq!(spans[1].content, "t");
    assert_eq!(spans[1].style.fg, Some(syntax_color));
    assert_eq!(spans[1].style.bg, Some(SELECT_BG));
    assert_eq!(spans[4].style.fg, Some(BG));
    assert_eq!(spans[4].style.bg, Some(TEXT));
}

#[test]
fn linewise_selection_does_not_highlight_the_diff_gutter() {
    let diff = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-old\n+new\n";
    let mut app = App::new(parse_unified_diff(diff), Vec::new());
    app.diff_pane.cursor = 2;
    app.key(KeyEvent::new(KeyCode::Char('V'), KeyModifiers::SHIFT));
    let line = app.current().lines[2].clone();

    let rendered =
        app.diff_pane
            .line_for_test(&app.session, &app.comment_editor, app.focus, &line, 2, 40);

    assert!(
        rendered.spans[..3]
            .iter()
            .all(|span| span.style.bg == Some(GREEN_BG))
    );
    assert!(
        rendered.spans[3..]
            .iter()
            .all(|span| span.style.bg == Some(SELECT_BG))
    );
}

#[test]
fn normal_mode_renders_and_moves_the_character_cursor() {
    let diff = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n+hello\n";
    let mut app = App::new(parse_unified_diff(diff), Vec::new());
    app.diff_pane.cursor = 1;
    app.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    app.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    let line = app.current().lines[1].clone();

    let rendered =
        app.diff_pane
            .line_for_test(&app.session, &app.comment_editor, app.focus, &line, 1, 40);
    let cursor = rendered
        .spans
        .iter()
        .find(|span| span.content == "l" && span.style.bg == Some(TEXT))
        .unwrap();

    assert_eq!(app.diff_pane.visual_col, 2);
    assert_eq!(cursor.style.fg, Some(GREEN_BG));
}

#[test]
fn tab_indented_go_lines_keep_their_indent_and_cursor_when_moving_down() {
    let diff = "diff --git a/main.go b/main.go\n--- a/main.go\n+++ b/main.go\n@@ -0,0 +1,2 @@\n+\tif ready {\n+\t\treturn\n";
    let mut app = App::new(parse_unified_diff(diff), Vec::new());
    let mut terminal = Terminal::new(TestBackend::new(100, 20)).unwrap();

    terminal.draw(|frame| app.draw(frame)).unwrap();
    let first_line = terminal.backend().buffer();
    assert_eq!(first_line.cell((49, 3)).unwrap().symbol(), " ");
    assert_eq!(first_line.cell((49, 3)).unwrap().bg, TEXT);
    assert!(
        (53..58).any(|x| first_line.cell((x, 3)).unwrap().symbol() == "i"),
        "the tab should create visible indentation before the Go code"
    );

    app.key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    terminal.draw(|frame| app.draw(frame)).unwrap();
    let second_line = terminal.backend().buffer();
    assert_eq!(second_line.cell((49, 4)).unwrap().symbol(), " ");
    assert_eq!(second_line.cell((49, 4)).unwrap().bg, TEXT);
    assert!(
        (57..66).any(|x| second_line.cell((x, 4)).unwrap().symbol() == "r"),
        "two tabs should create a larger visible indent"
    );
}

#[test]
fn diff_block_cursor_is_hidden_while_comment_editor_has_focus() {
    let diff = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n+hello\n";
    let mut app = App::new(parse_unified_diff(diff), Vec::new());
    app.diff_pane.cursor = 1;
    app.open_editor();
    let line = app.current().lines[1].clone();

    let rendered =
        app.diff_pane
            .line_for_test(&app.session, &app.comment_editor, app.focus, &line, 1, 40);

    assert!(
        rendered
            .spans
            .iter()
            .filter(|span| span.content.contains('h'))
            .all(|span| span.style.bg != Some(TEXT))
    );
}

#[test]
fn normal_cursor_is_visible_on_a_hunk_header() {
    let diff = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n line\n";
    let mut app = App::new(parse_unified_diff(diff), Vec::new());
    app.diff_pane.cursor = 0;
    let line = app.current().lines[0].clone();

    let rendered =
        app.diff_pane
            .line_for_test(&app.session, &app.comment_editor, app.focus, &line, 0, 40);
    let cursor = rendered
        .spans
        .iter()
        .find(|span| span.content == "@" && span.style.bg == Some(TEXT))
        .unwrap();

    assert_eq!(cursor.style.fg, Some(HUNK_BG));
}

#[test]
fn hunk_header_supports_characterwise_visual_selection_and_yank() {
    let diff = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n line\n";
    let mut app = App::new(parse_unified_diff(diff), Vec::new());
    app.diff_pane.cursor = 0;

    app.key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
    app.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    app.key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    let line = app.current().lines[0].clone();
    let rendered =
        app.diff_pane
            .line_for_test(&app.session, &app.comment_editor, app.focus, &line, 0, 40);
    assert!(
        rendered
            .spans
            .iter()
            .any(|span| span.style.bg == Some(TEXT))
    );
    let outcome = app.key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));

    assert!(matches!(outcome, Outcome::Yank(ref text) if text == "@@ "));
}

#[test]
fn c_selects_a_diff_range_for_commenting() {
    let diff = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1,2 +1,2 @@\n one\n two\n";
    let mut app = App::new(parse_unified_diff(diff), Vec::new());
    app.diff_pane.cursor = 1;

    app.key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE));
    app.key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    app.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    app.comment_editor.text = "Review both".into();
    app.comment_editor.cursor = app.comment_editor.text.len();
    app.save_editor();

    assert_eq!(app.session.comments[0].new_start, Some(1));
    assert_eq!(app.session.comments[0].new_end, Some(2));
    assert_eq!(app.session.comments[0].excerpt, " one\n two");

    app.statusline.clear_notice();
    let mut terminal = Terminal::new(TestBackend::new(100, 20)).unwrap();
    terminal.draw(|frame| app.draw(frame)).unwrap();
    let rendered = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(rendered.contains("Comment #1 · L1–2"));
    assert!(rendered.contains("Enter edit"));
}

#[test]
fn sidebar_spans_can_scroll_horizontally() {
    let spans = vec![
        Span::styled("  ", Style::default().fg(MUTED)),
        Span::styled("длинный.rs", Style::default().fg(TEXT)),
    ];
    let cropped = crop_spans(spans, 4, 5);
    assert_eq!(
        cropped
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>(),
        "инный"
    );
    assert_eq!(cropped[0].style.fg, Some(TEXT));
}

#[test]
fn sidebar_file_statuses_are_compact_and_color_coded() {
    let text = |status| {
        file_status_spans(status)
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    };

    assert_eq!(text(FileStatus::Added), "+ ");
    assert_eq!(text(FileStatus::Deleted), "− ");
    assert_eq!(text(FileStatus::Modified), "~ ");
    assert_eq!(text(FileStatus::Renamed), "→ ");
    let modified = file_status_spans(FileStatus::Modified);
    assert_eq!(modified[0].style.fg, Some(BLUE));
}

#[test]
fn space_marks_files_reviewed_and_advances_to_the_next_unreviewed_file() {
    let diff = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-old a\n+new a\ndiff --git a/b.rs b/b.rs\n--- a/b.rs\n+++ b/b.rs\n@@ -1 +1 @@\n-old b\n+new b\n";
    let mut app = App::new(parse_unified_diff(diff), Vec::new());

    app.key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
    assert!(app.session.reviewed_files.contains(&0));
    assert_eq!(app.diff_pane.file, 1);
    app.statusline.clear_notice();

    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal.draw(|frame| app.draw(frame)).unwrap();
    let rendered = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(rendered.contains("a.rs"));
    assert!(rendered.contains('✓'));
    assert!(rendered.contains("1 left"));
    assert!(rendered.contains("0 comments"));

    app.key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
    assert_eq!(app.session.reviewed_files.len(), 2);
    app.statusline.clear_notice();
    terminal.draw(|frame| app.draw(frame)).unwrap();
    let completed = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(completed.contains("done"));
    assert!(completed.contains("2 files"));
    assert!(completed.contains("Review complete"));

    assert_eq!(app.diff_pane.file, 1);
    app.key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
    assert!(!app.session.reviewed_files.contains(&1));
    assert_eq!(app.diff_pane.file, 1);
}

#[test]
fn inline_comment_is_a_full_width_visual_card() {
    let comment = Comment {
        id: "t-001".into(),
        path: "a.rs".into(),
        excerpt: " code".into(),
        old_start: Some(3),
        old_end: Some(3),
        new_start: Some(3),
        new_end: Some(4),
        anchor_old: Some(3),
        anchor_new: Some(4),
        text: "Please simplify\n```rust\nfix();\n```".into(),
    };
    let lines = inline_comment_lines(&comment, 1, 60);
    assert_eq!(lines.len(), 4);
    assert!(lines.iter().all(|line| line.width() == 60));
    assert_eq!(lines[0].spans[0].style.bg, Some(BG));
    assert!(
        lines[0]
            .spans
            .iter()
            .skip(1)
            .all(|span| span.style.bg == Some(COMMENT_BG))
    );
    assert_eq!(lines[0].spans[1].style.fg, Some(COMMENT));
    assert!(lines[0].spans[1].content.contains("Comment #1 · L3–4"));
}

#[test]
fn inline_comments_wrap_words_and_long_tokens_to_the_viewport() {
    let comment = Comment {
        id: "t-001".into(),
        path: "a.rs".into(),
        excerpt: " code".into(),
        old_start: Some(1),
        old_end: Some(1),
        new_start: Some(1),
        new_end: Some(1),
        anchor_old: Some(1),
        anchor_new: Some(1),
        text: "This comment is deliberately long enough to wrap without disappearing.\n012345678901234567890123456789".into(),
    };
    let lines = inline_comment_lines(&comment, 1, 42);

    assert!(lines.len() >= 4);
    assert!(lines.iter().all(|line| line.width() == 42));
    let rendered = lines
        .iter()
        .flat_map(|line| line.spans.iter())
        .map(|span| span.content.as_ref())
        .collect::<String>();
    assert!(rendered.contains("disappearing."));
    assert!(rendered.contains("0123456789"));
}

#[test]
fn editor_cursor_moves_to_the_new_line() {
    let diff = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-old\n+new\n";
    let mut app = App::new(parse_unified_diff(diff), Vec::new());
    app.open_editor();
    app.comment_editor.text = "first\n".into();
    app.comment_editor.cursor = app.comment_editor.text.len();
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal.draw(|frame| app.draw(frame)).unwrap();
    let buffer = terminal.backend().buffer();
    let mut text_row = None;
    let mut cursor_row = None;
    for y in 0..30 {
        for x in 0..100 {
            let cell = buffer.cell((x, y)).unwrap();
            match cell.symbol() {
                "f" if text_row.is_none() => text_row = Some(y),
                " " if cell.bg == TEXT => cursor_row = Some(y),
                _ => {}
            }
        }
    }
    assert!(cursor_row.unwrap() > text_row.unwrap());
}

#[test]
fn editor_word_wraps_long_input_without_changing_its_text() {
    let diff = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n line\n";
    let mut app = App::new(parse_unified_diff(diff), Vec::new());
    app.open_editor();
    app.comment_editor.text = "This reply is intentionally long and should wrap inside the inline editor without inserting newline characters into the saved text."
            .into();
    app.comment_editor.cursor = app.comment_editor.text.len();
    let original = app.comment_editor.text.clone();
    let mut lines = Vec::new();
    let mut map = Vec::new();

    app.diff_pane.editor_lines_for_test(
        &app.session,
        &app.comment_editor,
        app.focus,
        (&mut lines, &mut map),
        ("Reply", 52),
    );

    assert!(lines.len() >= 5);
    assert_eq!(app.comment_editor.text, original);
    assert_eq!(map.len(), lines.len());
    assert!(
        lines[1..lines.len() - 1]
            .iter()
            .all(|line| line.width() <= 52)
    );
}

#[test]
fn editor_arrows_move_cursor_between_lines() {
    let diff = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-old\n+new\n";
    let mut app = App::new(parse_unified_diff(diff), Vec::new());
    app.open_editor();
    app.comment_editor.text = "ab\ncde".into();
    app.comment_editor.cursor = app.comment_editor.text.len();

    app.editor_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    assert_eq!(app.comment_editor.cursor, 2);
    app.editor_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    assert_eq!(app.comment_editor.cursor, 1);
    app.editor_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    assert_eq!(app.comment_editor.cursor, 4);
    app.editor_key(KeyEvent::new(KeyCode::Char('λ'), KeyModifiers::NONE));
    assert_eq!(app.comment_editor.text, "ab\ncλde");
}

#[test]
fn terminal_shift_enter_inserts_newline_instead_of_j() {
    let diff = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-old\n+new\n";
    let mut app = App::new(parse_unified_diff(diff), Vec::new());
    app.open_editor();
    app.comment_editor.text = "Ready".into();
    app.comment_editor.cursor = app.comment_editor.text.len();
    app.editor_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL));
    assert_eq!(app.comment_editor.text, "Ready\n");
    assert_eq!(app.comment_editor.cursor, 6);
    assert!(app.session.comments.is_empty());
    assert_eq!(app.focus, Focus::Editor);
}

#[test]
fn sidebar_arrows_continue_after_selecting_a_comment() {
    let diff = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1,2 +1,2 @@\n one\n two\n";
    let comment = |line, text: &str| Comment {
        id: format!("t-{line:03}"),
        path: "a.rs".into(),
        excerpt: format!(" {text}"),
        old_start: Some(line),
        old_end: Some(line),
        new_start: Some(line),
        new_end: Some(line),
        anchor_old: Some(line),
        anchor_new: Some(line),
        text: text.into(),
    };
    let mut app = App::new(
        parse_unified_diff(diff),
        vec![comment(1, "one"), comment(2, "two")],
    );
    app.select_side_target(
        SideTarget::Comment {
            file: 0,
            comment: 0,
        },
        true,
    );

    app.key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));

    assert_eq!(
        app.file_tree.selection(),
        Some(SideTarget::Comment {
            file: 0,
            comment: 1
        })
    );
    assert_eq!(app.focus, Focus::Files);
}

#[test]
fn sidebar_scrolls_selected_file_into_view() {
    let diff = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-old\n+new\n";
    let template = parse_unified_diff(diff).remove(0);
    let files = (0..30)
        .map(|index| {
            let mut file = template.clone();
            file.path = format!("src/file_{index:02}.rs");
            file
        })
        .collect();
    let mut app = App::new(files, Vec::new());
    app.select_side_target(SideTarget::File(29), true);
    let mut terminal = Terminal::new(TestBackend::new(80, 14)).unwrap();

    terminal.draw(|frame| app.draw(frame)).unwrap();

    assert!(app.file_tree.scroll_y() > 0);
    assert!(
        app.file_tree
            .visible_targets()
            .contains(&Some(SideTarget::File(29)))
    );
}
