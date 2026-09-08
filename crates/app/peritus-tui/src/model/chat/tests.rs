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

mod model_selection;

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
fn enter_selects_every_advertised_model_for_every_role() {
    use peritus_app_protocol::{ProductModelCatalog, ProductModelInfo};
    let mut model = model();
    let profile = model.chat_providers().expect("providers").writer();
    let catalog = ProductModelCatalog::new(
        profile,
        "configured".to_owned(),
        [Some(true), Some(false), None]
            .into_iter()
            .enumerate()
            .map(|(index, tools)| {
                ProductModelInfo::new(format!("model-{index}"), format!("Model {index}"), tools)
                    .expect("model info")
            })
            .collect(),
        1,
        false,
        String::new(),
    )
    .expect("catalog");
    for role in ["writer", "reviewer", "fixer"] {
        for index in 0..3 {
            let _ = model.slash_command(&format!("/model {role}"));
            model.accept_model_catalog(catalog.clone());
            let _ = key(&mut model, KeyCode::Home);
            for _ in 0..index {
                let _ = key(&mut model, KeyCode::Down);
            }
            // A delayed cache reply must not change which row Enter selects.
            model.accept_model_catalog(catalog.clone());
            assert!(key(&mut model, KeyCode::Enter).is_empty());
            assert!(!model.chat.model_picker);
            let chosen = match role {
                "reviewer" => model.chat.models.reviewer(),
                "fixer" => model.chat.models.fixer(),
                _ => model.chat.models.writer(),
            };
            assert_eq!(chosen.id(), format!("model-{index}"));
            assert!(!chosen.manual());
        }
    }
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
fn control_c_exits_idle_chat_with_or_without_a_draft() {
    for draft in ["", "hello"] {
        let mut model = model();
        model.paste_chat(draft);
        let effects = model.update(Action::TerminalEvent(Event::Key(KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
        ))));
        assert!(matches!(effects.as_slice(), [Effect::Quit]));
        assert!(model.quitting);
        assert_eq!(model.chat.buffer, draft);
    }
}

#[test]
fn control_c_exits_disconnected_chat_and_the_model_picker() {
    let mut model = model();
    let _ = model.update(Action::Disconnected("lost".to_owned()));
    model.chat.model_picker = true;
    let effects = model.update(Action::TerminalEvent(Event::Key(KeyEvent::new(
        KeyCode::Char('c'),
        KeyModifiers::CONTROL,
    ))));
    assert!(matches!(effects.as_slice(), [Effect::Quit]));
    assert!(model.quitting);
}

#[test]
fn escape_returns_from_runs() {
    let mut model = model();
    model.paste_chat("/runs");
    let _ = key(&mut model, KeyCode::Enter);
    assert_eq!(model.view, View::Runs);
    let _ = key(&mut model, KeyCode::Esc);
    assert_eq!(model.view, View::Conversation);
}

#[test]
fn inspection_pages_scroll_and_home_returns_to_the_start() {
    let mut model = model();
    for view in [View::Diff, View::Review] {
        model.view = view;
        let _ = key(&mut model, KeyCode::PageDown);
        assert_eq!(model.product.as_ref().expect("product").inspection_scroll, 12);
        let _ = key(&mut model, KeyCode::PageUp);
        assert_eq!(model.product.as_ref().expect("product").inspection_scroll, 0);
        let _ = key(&mut model, KeyCode::PageDown);
        let _ = key(&mut model, KeyCode::Home);
        assert_eq!(model.product.as_ref().expect("product").inspection_scroll, 0);
    }
}

#[test]
fn stop_without_active_work_is_an_explained_local_noop() {
    let mut model = model();
    assert!(model.slash_command("/stop").is_empty());
    assert_eq!(model.notice.as_ref().expect("notice").text, "No active work to stop.");
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
    let effects = model.handle_chat_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert!(matches!(effects.as_slice(), [Effect::Quit]));
    assert!(model.quitting);
}

#[test]
fn editing_after_an_interrupt_resets_the_second_press_exit() {
    let mut model = model();
    model.paste_chat("hello");
    let _ = key(&mut model, KeyCode::Enter);
    let _ = model.handle_chat_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    let _ = key(&mut model, KeyCode::Char('x'));
    let effects = model.handle_chat_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert!(!effects.iter().any(|effect| matches!(effect, Effect::Quit)));
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

#[test]
fn folder_conversation_remains_available_without_git_candidate_commands() {
    let mut model = model();
    let product = model.product.as_mut().expect("product");
    product.launch = product.launch.clone().with_direct_folder(true);
    assert_eq!(model.direct_folder_chat(), Some(true));
    for command in ["/build create a file", "/commit", "/discard"] {
        assert!(model.slash_command(command).is_empty());
        assert_eq!(model.chat.mode, ProductInteractionMode::Chat);
    }
    model.paste_chat("Create the requested file here");
    assert!(key(&mut model, KeyCode::Enter).iter().any(|effect| matches!(effect, Effect::Send(AppMessage::Request(request)) if matches!(request.payload(), AppRequestPayload::Interact(value) if value.mode() == ProductInteractionMode::Chat))));
}

#[test]
fn folder_diff_opens_observed_scoped_evidence_without_a_git_handoff() {
    use peritus_app_protocol::{ProductRoleModels, ProductRunPhase, ProductRunSnapshot};
    let mut model = model();
    let product = model.product.as_mut().expect("product");
    product.launch = product.launch.clone().with_direct_folder(true);
    let run_id = RunId::new([0x42; 16]).expect("run");
    model.chat.run_id = Some(run_id);
    let snapshot = ProductRunSnapshot::new(
        run_id,
        WorkspaceId::new([4; 16]).expect("workspace"),
        model.chat_providers().expect("providers"),
        ProductRunPhase::Complete,
        1,
        "Update note.txt".to_owned(),
        "Complete in place".to_owned(),
        "--- before/note.txt\n+++ current/note.txt".to_owned(),
        "PASS".to_owned(),
        "Reviewed".to_owned(),
        "Updated".to_owned(),
    )
    .expect("snapshot");
    model.accept_chat(
        ProductInteractionSnapshot::new(
            snapshot,
            ProductInteractionMode::Chat,
            ProductRoleModels::default(),
            1,
            1,
            Vec::new(),
            None,
        )
        .expect("interaction"),
    );
    assert!(model.slash_command("/diff").is_empty());
    assert_eq!(model.view, View::Diff);
    assert!(model.slash_command("/discard").is_empty());
}
