//! Child navigation must use registered paths and preserve the running daemon configuration.

use super::*;
use crate::{AppLayout, ProductBootstrap};
use peritus_app_protocol::ConversationId;
use peritus_product_state::WorkspaceProfile;
use peritus_types::WorkspaceId;

fn workspace(root: &std::path::Path, id: &str) -> WorkspaceProfile {
    WorkspaceProfile::restricted(
        root.join(format!("source-{id}")).to_string_lossy().into_owned(),
        id.repeat(32),
        id.repeat(16),
        id.repeat(16),
        id.repeat(16),
        id.repeat(16),
    )
    .unwrap()
    .trust(
        root.join(format!("registration-{id}")).to_string_lossy().into_owned(),
        id.repeat(32),
        root.join(format!("managed-{id}")).to_string_lossy().into_owned(),
        root.join(format!("transactions-{id}")).to_string_lossy().into_owned(),
    )
    .unwrap()
}

#[test]
fn child_context_uses_its_registered_root_without_reconfiguring_or_selecting_workspace() {
    let temporary = tempfile::tempdir().unwrap();
    let layout = AppLayout::for_test(temporary.path()).prepare().unwrap();
    let child = workspace(temporary.path(), "42");
    ProductBootstrap::new(layout.clone()).configure_workspace(child.clone()).unwrap();
    let prepared = ProductBootstrap::new(layout.clone())
        .configure_workspace(workspace(temporary.path(), "41"))
        .unwrap();
    let config = std::fs::read(prepared.daemon_config_path()).unwrap();
    let generation = prepared.state().generation();
    let query = WorkbenchQuery::new(
        ConversationId::new([0x43; 16]).unwrap(),
        WorkspaceId::new([0x42; 16]).unwrap(),
    );
    let context = registered_context(&prepared, query).unwrap();
    assert_eq!(context.workspace_label(), child.managed_root().unwrap());
    assert_eq!(context.workspace_id(), query.workspace());
    assert_eq!(context.conversation(), Some(query));
    assert!(context.run_id().is_none());
    assert!(context.direct_folder_writable().is_none());
    let after = ProductBootstrap::new(layout).prepare().unwrap();
    assert_eq!(after.state().generation(), generation);
    assert_eq!(after.state().workspaces().active().unwrap().workspace_id(), "41".repeat(16));
    assert_eq!(std::fs::read(after.daemon_config_path()).unwrap(), config);
    assert_eq!(after.endpoint_path(), prepared.endpoint_path());
}

#[test]
fn unregistered_child_does_not_borrow_parent_authority() {
    let temporary = tempfile::tempdir().unwrap();
    let layout = AppLayout::for_test(temporary.path()).prepare().unwrap();
    let prepared = ProductBootstrap::new(layout)
        .configure_workspace(workspace(temporary.path(), "41"))
        .unwrap();
    let query = WorkbenchQuery::new(
        ConversationId::new([0x43; 16]).unwrap(),
        WorkspaceId::new([0x42; 16]).unwrap(),
    );
    assert!(
        registered_context(&prepared, query)
            .unwrap_err()
            .to_string()
            .contains("not in this launcher's registered configuration")
    );
}

#[test]
fn active_folder_can_be_reopened_but_inactive_unregistered_folders_cannot() {
    let temporary = tempfile::tempdir().unwrap();
    let layout = AppLayout::for_test(temporary.path()).prepare().unwrap();
    let folder = |id: &str| {
        WorkspaceProfile::restricted(
            temporary.path().join(id).to_string_lossy().into_owned(),
            id.repeat(32),
            id.repeat(16),
            id.repeat(16),
            id.repeat(16),
            id.repeat(16),
        )
        .unwrap()
        .into_direct_folder()
        .unwrap()
        .trust_folder()
        .unwrap()
    };
    ProductBootstrap::new(layout.clone()).configure_workspace(folder("45")).unwrap();
    let prepared = ProductBootstrap::new(layout).configure_workspace(folder("46")).unwrap();
    let before = std::fs::read(prepared.daemon_config_path()).unwrap();
    let active = WorkspaceId::new([0x46; 16]).unwrap();
    let context = configured_context(&prepared, active)
        .unwrap()
        .with_run(Some(RunId::new([0x47; 16]).unwrap()));
    assert_eq!(context.workspace_id(), active);
    assert_eq!(context.direct_folder_writable(), Some(true));
    assert!(context.workspace_label().ends_with("46"));
    assert!(configured_context(&prepared, WorkspaceId::new([0x45; 16]).unwrap()).is_err());
    assert_eq!(std::fs::read(prepared.daemon_config_path()).unwrap(), before);
}
