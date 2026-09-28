//! Product-owned daemon recovery routing.

use peritus_types::{ProviderProfileId, WorkspaceId};

use super::*;

mod drafts;
mod navigation;

#[test]
fn late_disconnect_from_replaced_connection_does_not_take_new_connection_offline() {
    use peritus_app_protocol::{AppProtocolLimits, ProtocolContext, ProtocolVersion};
    let old = ProtocolContext::new(
        ProtocolId::new([1; 16]).unwrap(),
        ProtocolVersion::new(1, 0).unwrap(),
        SessionId::new([3; 16]).unwrap(),
    );
    let current =
        ProtocolContext::new(ProtocolId::new([2; 16]).unwrap(), old.version(), old.session_id());
    let mut model = AppModel::with_product([8; 32], None);
    model.update(Action::Connected {
        context: current,
        limits: AppProtocolLimits::PRODUCTION,
        server: "reconnected fixture".into(),
        downgraded: false,
    });
    model.chat.buffer = "retained draft".into();
    let late = ClientEvent::Disconnected { context: old, error: "old socket closed".into() };
    if let Some(action) = client_action(late, model.protocol_context()) {
        model.update(action);
    }
    assert_eq!(
        model.protocol_context(),
        Some(current),
        "an old socket must not tear down the new session"
    );
    assert_eq!(model.chat.buffer, "retained draft");
    let real =
        ClientEvent::Disconnected { context: current, error: "current socket closed".into() };
    model.update(client_action(real, model.protocol_context()).expect("current disconnect"));
    assert!(model.protocol_context().is_none());
    assert_eq!(model.chat.buffer, "retained draft");
}

#[test]
fn product_reconnect_returns_control_to_the_daemon_supervisor() {
    let product = ProductLaunchContext::new(
        WorkspaceId::new([81; 16]).expect("workspace"),
        "/managed/project".to_owned(),
        vec![ProductProviderOption::new(
            ProviderProfileId::new([82; 16]).expect("provider"),
            "Codex",
        )],
        Some(0),
    )
    .expect("product context");
    let config = TuiConfig::new("/unreachable/peritus.sock").with_product(product.clone());
    let mut model = AppModel::with_product([83; 32], Some(product));
    let (events, _receiver) = mpsc::channel(1);
    let mut connection = Connection::new(events);

    let flow = apply_effects(
        vec![Effect::Reconnect],
        &config,
        &mut model,
        &mut connection,
        &mut LocalReads::default(),
    );

    assert!(matches!(flow, ControlFlow::RecoverDaemon));
    assert!(!connection.active(), "the stale endpoint must not be reopened directly");
}

#[test]
fn recovery_retains_conversation_selection_and_draft_only_for_the_same_configuration() {
    use peritus_app_protocol::{ProductModelChoice, ProductRoleModels};
    use peritus_types::RunId;
    let config = TuiConfig::new("/fixture/peritus.sock");
    let mut state = TuiState::default();
    let mut model = state.take_model(&config, [11; 32]);
    model.chat.buffer = "unsent draft λ".to_owned();
    model.chat.cursor = model.chat.buffer.len();
    model.chat.run_id = Some(RunId::new([12; 16]).expect("run"));
    let choice = ProductModelChoice::new("advertised-model".to_owned(), false).expect("model");
    model.chat.models = ProductRoleModels::new(choice.clone(), choice.clone(), choice);
    state.retain(config.clone(), model);
    let restored = state.take_model(&config, [13; 32]);
    assert_eq!(restored.chat.buffer, "unsent draft λ");
    assert_eq!(restored.chat.cursor, restored.chat.buffer.len());
    assert_eq!(restored.chat.run_id, Some(RunId::new([12; 16]).expect("run")));
    assert_eq!(restored.chat.models.writer().id(), "advertised-model");
    state.retain(config, restored);
    let different = state.take_model(&TuiConfig::new("/different/peritus.sock"), [14; 32]);
    assert!(different.chat.buffer.is_empty());
    assert!(different.chat.run_id.is_none());
    assert!(different.chat.models.writer().id().is_empty());
}

#[test]
fn explicit_browser_run_is_selected_without_starting_work() {
    use peritus_types::RunId;
    let run = RunId::new([91; 16]).expect("run");
    let product = ProductLaunchContext::new(
        WorkspaceId::new([92; 16]).expect("workspace"),
        "fixture".into(),
        vec![ProductProviderOption::new(
            ProviderProfileId::new([93; 16]).expect("provider"),
            "Fixture",
        )],
        Some(0),
    )
    .expect("context")
    .with_run(Some(run));
    let config = TuiConfig::new("/fixture/peritus.sock").with_product(product);
    let mut state = TuiState::default();
    let model = state.take_model(&config, [94; 32]);
    assert_eq!(model.chat.run_id, Some(run));
    assert!(model.chat.snapshot.is_none());
    assert!(model.chat.buffer.is_empty());
    state.retain(config.clone(), model);
    assert_eq!(state.take_model(&config, [95; 32]).chat.run_id, Some(run));
}
