//! Product-owned daemon recovery routing.

use peritus_types::{ProviderProfileId, WorkspaceId};

use super::*;

#[tokio::test]
async fn product_reconnect_returns_control_to_the_daemon_supervisor() {
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
    let mut client = None;
    let (events, _receiver) = mpsc::channel(1);
    let mut generation = 0;

    let flow = apply_effects(
        vec![Effect::Reconnect],
        &config,
        &mut model,
        &mut client,
        &events,
        &mut generation,
    )
    .await
    .expect("reconnect routing");

    assert!(matches!(flow, ControlFlow::RecoverDaemon));
    assert_eq!(generation, 0, "the stale endpoint must not be reopened directly");
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
