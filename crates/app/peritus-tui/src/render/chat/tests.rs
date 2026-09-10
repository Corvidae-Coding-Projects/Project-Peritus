use super::*;
use peritus_app_protocol::{
    ProductActivity, ProductInteractionMode, ProductInteractionSnapshot, ProductProviderSelection,
    ProductRoleModels, ProductRunPhase, ProductRunSnapshot,
};
use peritus_types::{ProviderProfileId, RunId, WorkspaceId};
use ratatui::{Terminal, backend::TestBackend};

fn model() -> AppModel {
    let mut model = AppModel::new([1; 32]);
    model.connection = ConnectionStatus::Online { server: "test".to_owned(), downgraded: false };
    let profile = ProviderProfileId::new([2; 16]).expect("profile");
    let snapshot = ProductRunSnapshot::new(
        RunId::new([3; 16]).expect("run"),
        WorkspaceId::new([4; 16]).expect("workspace"),
        ProductProviderSelection::new(profile, profile, profile),
        ProductRunPhase::Writing,
        1,
        "question".to_owned(),
        "Responding".to_owned(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
    )
    .expect("run");
    let activities = (1..=30)
        .map(|sequence| {
            ProductActivity::new(
                sequence,
                ProductActivityKind::Assistant,
                format!("Activity {sequence:02}: public reply"),
                String::new(),
            )
            .expect("activity")
        })
        .collect();
    model.chat.run_id = Some(snapshot.run_id());
    model.chat.snapshot = Some(
        ProductInteractionSnapshot::new(
            snapshot,
            ProductInteractionMode::Chat,
            ProductRoleModels::default(),
            2,
            1,
            activities,
            None,
        )
        .expect("interaction"),
    );
    model
}

fn screen(model: &AppModel, width: u16, height: u16) -> (String, (u16, u16)) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
    let frame = terminal.draw(|frame| draw(frame, model)).expect("render");
    let text = frame.buffer.content().iter().map(ratatui::buffer::Cell::symbol).collect::<String>();
    let cursor = terminal.get_cursor_position().expect("composer cursor");
    (text, (cursor.x, cursor.y))
}

#[test]
fn narrow_and_wide_screens_keep_composer_and_input_status_visible() {
    let mut model = model();
    model.chat.buffer = "draft λ 界 ".repeat(40);
    model.chat.buffer.push_str("CURSOR-END");
    model.chat.cursor = model.chat.buffer.len();
    for width in [40, 80] {
        let (text, cursor) = screen(&model, width, 24);
        assert!(text.contains("CURSOR-END"));
        assert!(text.contains("Message / steer active work"));
        assert!(text.contains("received 2"));
        assert!(cursor.0 > 0 && cursor.0 < width - 1);
        assert!(cursor.1 > 0 && cursor.1 < 23);
    }
    assert!(model.chat.buffer.ends_with("CURSOR-END"), "render must not consume the draft");
}

#[test]
fn transcript_scroll_reaches_earlier_activity_beyond_the_latest_twelve() {
    let mut model = model();
    let (tail, _) = screen(&model, 80, 24);
    assert!(tail.contains("Activity 30"));
    assert!(!tail.contains("Activity 01"));
    model.chat.scroll = usize::MAX;
    let (head, _) = screen(&model, 80, 24);
    assert!(head.contains("Activity 01"));
    assert!(!head.contains("Activity 30"));
}

#[test]
fn command_picker_keeps_first_and_last_selection_visible() {
    let mut model = model();
    model.chat.buffer = "/".to_owned();
    model.chat.cursor = 1;
    for (selection, command) in [(0, "/chat"), (crate::model::chat::COMMANDS.len() - 1, "/quit")] {
        model.chat.command_selection = selection;
        let (text, _) = screen(&model, 80, 24);
        assert!(text.contains(&format!("▸ {command}")), "selected command hidden: {text}");
    }
}

#[test]
fn effort_picker_and_entrypoint_are_visible_on_narrow_and_wide_terminals() {
    let mut model = model();
    for width in [40, 100] {
        let (ordinary, _) = screen(&model, width, 24);
        assert!(ordinary.contains("/effort"), "effort control must be discoverable");
        model.chat.show_effort_picker();
        for selection in [0, peritus_app_protocol::ProductModelEffort::ALL.len() - 1] {
            model.chat.effort_selection = selection;
            let (picker, _) = screen(&model, width, 24);
            assert!(picker.contains("Reasoning effort for chat/writer"));
            assert!(picker.contains(&format!(
                "▸ {}",
                peritus_app_protocol::ProductModelEffort::ALL[selection].label()
            )));
            assert!(picker.contains("Enter saves"));
        }
        model.chat.close_effort_picker();
    }
}

fn conversation_with_diagnostics() -> AppModel {
    let mut model = model();
    let previous = model.chat.snapshot.take().expect("snapshot");
    let activities = [
        (ProductActivityKind::User, "Fix the parser.", ""),
        (ProductActivityKind::Status, "I'm working on your reply.", ""),
        (ProductActivityKind::Status, "Requesting model gpt-5.6-sol", ""),
        (ProductActivityKind::Assistant, "I'll inspect the parser.", ""),
        (
            ProductActivityKind::Tool,
            "Called workspace_read path=parser.rs",
            "Parser source preview",
        ),
        (ProductActivityKind::Status, "I'm inspecting the workspace and preparing the design.", ""),
        (ProductActivityKind::Assistant, "The parser drops empty input.", ""),
        (ProductActivityKind::Error, "Connection lost; the result is not verified.", ""),
    ]
    .into_iter()
    .zip(1..)
    .map(|((kind, text, detail), sequence)| {
        ProductActivity::new(sequence, kind, text.to_owned(), detail.to_owned()).expect("activity")
    })
    .collect();
    model.chat.snapshot = Some(
        ProductInteractionSnapshot::new(
            previous.snapshot().clone(),
            previous.mode(),
            previous.models().clone(),
            previous.received(),
            previous.incorporated(),
            activities,
            None,
        )
        .expect("snapshot"),
    );
    model
}

#[test]
fn ordinary_transcript_shows_conversation_and_errors_without_harness_chatter() {
    let model = conversation_with_diagnostics();
    for width in [60, 100] {
        let (text, _) = screen(&model, width, 32);
        for visible in [
            "Fix the parser.",
            "I'll inspect the parser.",
            "The parser drops empty input.",
            "Connection lost;",
            "Called workspace_read path=parser.rs",
            "Parser source preview",
        ] {
            assert!(text.contains(visible), "missing {visible}: {text}");
        }
        for diagnostic in [
            "I'm working on your reply.",
            "Requesting model",
            "Reading the relevant file",
            "Finished that step.",
            "preparing the design",
        ] {
            assert!(!text.contains(diagnostic), "harness chatter visible: {diagnostic}");
        }
    }
}

#[test]
fn explicit_details_reveal_host_activity_with_distinct_labels() {
    let mut model = conversation_with_diagnostics();
    model.chat.expanded = true;
    let (expanded, _) = screen(&model, 100, 48);
    for diagnostic in [
        "Status",
        "Requesting model gpt-5.6-sol",
        "Called workspace_read path=parser.rs",
        "Parser source preview",
    ] {
        assert!(expanded.contains(diagnostic), "missing diagnostic: {diagnostic}");
    }
    assert_eq!(expanded.matches("Peritus").count(), 3, "app title plus two model reply labels");
    model.chat.expanded = false;
    let (collapsed, _) = screen(&model, 100, 32);
    assert!(!collapsed.contains("Requesting model"));
    assert!(collapsed.contains("I'll inspect the parser."));
}

#[test]
fn tool_entries_show_actual_commands_and_bounded_sanitized_results_without_details() {
    let activity = ProductActivity::new(
        1, ProductActivityKind::Tool, "Ran cargo test".to_owned(),
        "Exit code: 1\nstdout:\nfirst line\nline 2\nline 3\nline 4\nline 5\nline 6\nline 7\nstderr:\n\u{1b}[31massertion failed\u{1b}[0m".to_owned(),
    ).unwrap();
    for width in [40, 100] {
        let mut compact = Vec::new();
        append_tool(&mut compact, &activity, false, width);
        let compact = compact.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n");
        assert!(compact.contains("• Ran cargo test"));
        assert!(compact.contains("Exit code: 1"));
        assert!(compact.contains("assertion failed"));
        assert!(compact.contains("more lines · /details"));
        assert!(!compact.contains("line 4"));
        assert!(!compact.contains('\u{1b}'));
        let mut expanded = Vec::new();
        append_tool(&mut expanded, &activity, true, width);
        let expanded = expanded.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n");
        assert!(expanded.contains("line 4"));
        assert!(!expanded.contains("more lines"));
    }
}

#[test]
fn one_working_idler_updates_elapsed_seconds_without_adding_transcript_rows() {
    use crate::action::Action;
    use std::time::{Duration, Instant};
    let mut model = conversation_with_diagnostics();
    let started = Instant::now();
    let _ = model.update(Action::Tick(started));
    let (initial, _) = screen(&model, 100, 32);
    assert!(initial.contains("*working (0s)"));
    let _ = model.update(Action::Tick(started + Duration::from_secs(40)));
    let (later, _) = screen(&model, 100, 32);
    assert!(later.contains("*working (40s)"));
    assert_eq!(later.matches("*working").count(), 1);
    assert!(!later.contains("*working (0s)"));
    assert!(!later.contains("Requesting model"));
    assert!(later.contains("I'll inspect the parser."));
    let _ = model.update(Action::Disconnected("socket closed".to_owned()));
    let (disconnected, _) = screen(&model, 100, 32);
    assert!(!disconnected.contains("*working"), "disconnection cannot imply live progress");
}

#[test]
fn working_indicator_is_bold_white_directly_above_the_composer() {
    use crate::action::Action;
    use std::time::{Duration, Instant};
    let mut model = model();
    let started = Instant::now();
    let _ = model.update(Action::Tick(started));
    let _ = model.update(Action::Tick(started + Duration::from_secs(40)));
    for (width, height, draft) in [(40, 12, ""), (100, 32, ""), (40, 24, "one\ntwo\nthree")] {
        model.chat.buffer = draft.to_owned();
        model.chat.cursor = draft.len();
        for expanded in [false, true] {
            model.chat.expanded = expanded;
            for scroll in [0, usize::MAX] {
                model.chat.scroll = scroll;
                let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
                let frame = terminal.draw(|frame| draw(frame, &model)).unwrap();
                let label = "*working (40s)";
                let composer_row = (0..height)
                    .find(|row| {
                        (0..width)
                            .map(|column| frame.buffer[(column, *row)].symbol())
                            .collect::<String>()
                            .contains("Message / steer active work")
                    })
                    .expect("composer must remain visible");
                let indicator_row = composer_row.checked_sub(1).expect("row above composer");
                for (column, character) in label.chars().enumerate() {
                    let cell = &frame.buffer[(u16::try_from(column).unwrap(), indicator_row)];
                    assert_eq!(cell.symbol(), character.to_string());
                    assert_eq!(cell.fg, Color::White);
                    assert!(cell.modifier.contains(Modifier::BOLD));
                }
                let text = frame
                    .buffer
                    .content()
                    .iter()
                    .map(ratatui::buffer::Cell::symbol)
                    .collect::<String>();
                assert_eq!(text.matches("*working").count(), 1, "no footer duplicate");
                let activity = if scroll == 0 { "Activity 30" } else { "Activity 01" };
                assert!(text.find(activity).unwrap() < text.find(label).unwrap());
                assert!(text.contains("Message / steer active work"));
            }
        }
    }
}

#[test]
fn working_idler_stops_at_every_idle_or_terminal_phase() {
    use crate::action::Action;
    use std::time::{Duration, Instant};
    for phase in [
        ProductRunPhase::WaitingForUser,
        ProductRunPhase::Complete,
        ProductRunPhase::Failed,
        ProductRunPhase::Cancelled,
        ProductRunPhase::RecoveryRequired,
    ] {
        let mut model = model();
        let now = Instant::now();
        let _ = model.update(Action::Tick(now));
        assert_eq!(model.chat.working.elapsed_seconds(), Some(0));
        let previous = model.chat.snapshot.take().expect("snapshot");
        let current = previous.snapshot();
        let snapshot = ProductRunSnapshot::new(
            current.run_id(),
            current.workspace_id(),
            current.providers(),
            phase,
            current.cycle(),
            current.task().to_owned(),
            "Stopped".to_owned(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        )
        .expect("phase snapshot");
        model.chat.snapshot = Some(
            ProductInteractionSnapshot::new(
                snapshot,
                previous.mode(),
                previous.models().clone(),
                previous.received(),
                previous.incorporated(),
                previous.activities().to_vec(),
                None,
            )
            .expect("interaction"),
        );
        let _ = model.update(Action::Tick(now + Duration::from_secs(40)));
        assert_eq!(model.chat.working.elapsed_seconds(), None, "{phase:?}");
        assert!(!screen(&model, 100, 32).0.contains("*working"));
    }
}
