use super::*;
use peritus_app_protocol::{
    ProductActivity, ProductInteractionMode, ProductInteractionSnapshot, ProductProviderSelection,
    ProductRoleModels, ProductRunPhase, ProductRunSnapshot,
};
use peritus_types::{ProviderProfileId, RunId, WorkspaceId};
use ratatui::{Terminal, backend::TestBackend};

fn model() -> AppModel {
    let mut model = AppModel::new([1; 32]);
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
