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
