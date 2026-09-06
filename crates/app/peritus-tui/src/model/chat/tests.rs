use super::*;
use crate::{
    action::Action,
    model::View,
    runtime::{ProductLaunchContext, ProductProviderOption},
};
use crossterm::event::Event;
use peritus_app_protocol::{
    AppMessage, AppProtocolLimits, ProtocolContext, ProtocolId, ProtocolVersion,
};
use peritus_types::{ProviderProfileId, SessionId, WorkspaceId};

fn model() -> AppModel {
    let launch = ProductLaunchContext::new(
        WorkspaceId::new([4; 16]).expect("workspace"),
        "fixture".to_owned(),
        vec![ProductProviderOption::new(
            ProviderProfileId::new([5; 16]).expect("profile"),
            "Fixture",
        )],
        Some(0),
    )
    .expect("launch");
    let mut model = AppModel::with_product([6; 32], Some(launch));
    let _ = model.update(Action::Connected {
        context: ProtocolContext::new(
            ProtocolId::new([1; 16]).expect("protocol"),
            ProtocolVersion::new(1, 0).expect("version"),
            SessionId::new([2; 16]).expect("session"),
        ),
        limits: AppProtocolLimits::PRODUCTION,
        server: "fixture".to_owned(),
        downgraded: false,
    });
    model
}
fn key(model: &mut AppModel, code: KeyCode) -> Vec<Effect> {
    model.update(Action::TerminalEvent(Event::Key(KeyEvent::new(code, KeyModifiers::NONE))))
}

#[test]
fn default_composer_types_hotkey_letters_and_submits_chat_not_a_build() {
    let mut model = model();
    assert_eq!(model.view, View::Conversation);
    for character in "what is this? λ".chars() {
        assert!(key(&mut model, KeyCode::Char(character)).is_empty());
    }
    assert!(model.editor.is_none());
    let effects = key(&mut model, KeyCode::Enter);
    assert!(effects.iter().any(|effect| matches!(effect, Effect::Send(AppMessage::Request(request)) if matches!(request.payload(), AppRequestPayload::Interact(interaction) if interaction.mode() == ProductInteractionMode::Chat && interaction.request().task() == "what is this? λ"))));
    assert!(model.chat.buffer.is_empty());
}

#[test]
fn unknown_commands_preserve_draft_and_completion_is_local() {
    let mut model = model();
    model.paste_chat("/nonsense");
    assert!(key(&mut model, KeyCode::Enter).is_empty());
    assert_eq!(model.chat.buffer, "/nonsense");
    model.chat.buffer = "/pla".to_owned();
    model.chat.cursor = 4;
    assert!(key(&mut model, KeyCode::Tab).is_empty());
    assert_eq!(model.chat.buffer, "/plan ");
    model.paste_chat("inspect the options");
    let effects = key(&mut model, KeyCode::Enter);
    assert!(effects.iter().any(|effect| matches!(effect, Effect::Send(AppMessage::Request(request)) if matches!(request.payload(), AppRequestPayload::Interact(interaction) if interaction.mode() == ProductInteractionMode::Plan))));
}

#[test]
fn disconnected_submission_and_unknown_delivery_retain_user_text() {
    let mut model = model();
    model.paste_chat("hello");
    let _ = key(&mut model, KeyCode::Enter);
    let _ = model.update(Action::Disconnected("lost".to_owned()));
    assert_eq!(model.chat.buffer, "hello");
    assert!(key(&mut model, KeyCode::Enter).is_empty());
    assert_eq!(model.chat.buffer, "hello");
}

#[test]
fn control_c_clears_an_idle_draft_without_quitting_and_escape_returns_from_runs() {
    let mut model = model();
    model.paste_chat("hello");
    let effects = model.handle_chat_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert!(effects.is_empty());
    assert!(!model.quitting);
    assert!(model.chat.buffer.is_empty());
    model.paste_chat("/runs");
    let _ = key(&mut model, KeyCode::Enter);
    assert_eq!(model.view, View::Runs);
    let _ = key(&mut model, KeyCode::Esc);
    assert_eq!(model.view, View::Conversation);
}

#[test]
fn stop_during_initial_submission_targets_the_exact_conversation() {
    let mut model = model();
    model.paste_chat("hello");
    let _ = key(&mut model, KeyCode::Enter);
    let run_id = model.chat.run_id.expect("pending conversation");
    model.paste_chat("retain this steering draft");
    let effects = model.handle_chat_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert!(effects.iter().any(|effect| matches!(effect,
        Effect::Send(AppMessage::Request(request)) if matches!(request.payload(),
            AppRequestPayload::ControlProductRun(control) if control.run_id() == run_id
                && control.action() == ProductRunControlAction::Cancel))));
    assert_eq!(model.chat.buffer, "retain this steering draft");
    assert!(!model.quitting);
}

#[test]
fn candidate_commands_without_an_observed_chat_run_fail_closed() {
    let mut model = model();
    model.chat.run_id = Some(RunId::new([9; 16]).expect("unobserved run"));
    for command in ["/accept", "/commit", "/export", "/discard", "/run", "/diff"] {
        assert!(model.slash_command(command).is_empty());
        assert_eq!(model.view, View::Conversation);
    }
}
