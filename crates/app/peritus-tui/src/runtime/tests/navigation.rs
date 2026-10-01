//! Launcher handoff selects only the matching workspace and never starts work on entry.

use super::*;
use peritus_app_protocol::{
    AppMessage, AppProtocolLimits, AppRequestPayload, ConversationId, ProtocolContext,
    ProtocolFeatureName, ProtocolVersion, WellKnownProtocolFeature, WorkbenchQuery,
};

fn product(workspace: WorkspaceId) -> ProductLaunchContext {
    ProductLaunchContext::new(workspace, "registered child".into(), Vec::new(), None).unwrap()
}

#[test]
fn launcher_child_selection_loads_metadata_without_starting_or_inheriting_parent_work() {
    let workspace = WorkspaceId::new([65; 16]).unwrap();
    let query = WorkbenchQuery::new(ConversationId::new([66; 16]).unwrap(), workspace);
    let other = WorkspaceId::new([67; 16]).unwrap();
    assert!(product(other).with_conversation(query).is_err());
    let context = product(workspace)
        .with_run(Some(peritus_types::RunId::new([68; 16]).unwrap()))
        .with_conversation(query)
        .unwrap();
    let config = TuiConfig::new("fixture.sock").with_product(context);
    let mut state = TuiState::default();
    let mut model = state.take_model(&config, [69; 32]);
    assert_eq!(model.chat.workbench.selected, Some(query));
    assert!(model.chat.run_id.is_none());
    assert!(model.chat.workbench.snapshot.is_none());
    let protocol = ProtocolContext::new(
        ProtocolId::new([70; 16]).unwrap(),
        ProtocolVersion::new(1, 0).unwrap(),
        SessionId::new([71; 16]).unwrap(),
    );
    model.update(Action::Connected {
        context: protocol,
        limits: AppProtocolLimits::PRODUCTION,
        server: "fixture".into(),
        downgraded: false,
    });
    let effects = model.update(Action::NegotiatedFeatures {
        context: protocol,
        features: vec![
            ProtocolFeatureName::well_known(WellKnownProtocolFeature::WorkbenchControl).unwrap(),
        ],
    });
    assert!(matches!(effects.as_slice(), [Effect::Send(AppMessage::Request(request))]
        if matches!(request.payload(), AppRequestPayload::QueryWorkbench(actual) if *actual == query)));
}

#[test]
fn failed_child_handoff_retains_the_parent_and_its_unsent_draft_without_recreating_the_fork() {
    let config =
        TuiConfig::new("fixture.sock").with_product(product(WorkspaceId::new([72; 16]).unwrap()));
    let mut state = TuiState::default();
    let mut model = state.take_model(&config, [73; 32]);
    model.chat.buffer = "new draft while fork was pending".into();
    let query = WorkbenchQuery::new(
        ConversationId::new([74; 16]).unwrap(),
        WorkspaceId::new([75; 16]).unwrap(),
    );
    let (events, _receiver) = mpsc::channel(1);
    let mut connection = Connection::new(events);
    let flow = apply_effects(
        vec![Effect::OpenConversation(query)],
        &config,
        &mut model,
        &mut connection,
        &mut LocalReads::default(),
    );
    assert!(matches!(flow, ControlFlow::OpenConversation(actual) if actual == query));
    assert!(!connection.active());
    state.retain(config.clone(), model);
    state.conversation_open_failed("workspace is no longer registered");
    let model = state.take_model(&config, [76; 32]);
    assert_eq!(model.chat.buffer, "new draft while fork was pending");
    assert_eq!(
        model.product.as_ref().unwrap().launch.workspace_id(),
        WorkspaceId::new([72; 16]).unwrap()
    );
    assert!(model.chat.workbench.unresolved.is_none());
    assert!(model.chat.workbench.message.contains("Could not open the requested conversation"));
}

#[test]
fn run_handoff_opens_the_exact_target_and_failed_navigation_is_visible_from_runs() {
    let parent = WorkspaceId::new([77; 16]).unwrap();
    let child = WorkspaceId::new([78; 16]).unwrap();
    let run = peritus_types::RunId::new([79; 16]).unwrap();
    let config = TuiConfig::new("fixture.sock").with_product(product(parent));
    let mut state = TuiState::default();
    let mut model = state.take_model(&config, [80; 32]);
    model.view = crate::model::View::Runs;
    let (events, _receiver) = mpsc::channel(1);
    let mut connection = Connection::new(events);
    let flow = apply_effects(
        vec![Effect::OpenRun { run, workspace: child }],
        &config,
        &mut model,
        &mut connection,
        &mut LocalReads::default(),
    );
    assert!(
        matches!(flow, ControlFlow::OpenRun { run: actual, workspace } if actual == run && workspace == child)
    );
    state.retain(config.clone(), model);
    state.conversation_open_failed("the saved workspace is unavailable");
    let restored = state.take_model(&config, [81; 32]);
    assert_eq!(restored.view, crate::model::View::Conversation);
    assert!(restored.chat.workbench.open);
    assert_eq!(restored.product.as_ref().unwrap().launch.workspace_id(), parent);
    state.retain(config, restored);
    let child_config =
        TuiConfig::new("fixture.sock").with_product(product(child).with_run(Some(run)));
    let selected = state.take_model(&child_config, [82; 32]);
    assert_eq!(selected.chat.run_id, Some(run));
    assert!(selected.chat.workbench.selected.is_none());
    assert!(selected.chat.buffer.is_empty());
    assert_eq!(selected.product.as_ref().unwrap().launch.workspace_id(), child);
}
