//! Exact checkpoint lineage and fail-closed fork command coverage.

use super::*;
use peritus_app_protocol::{
    WorkbenchCheckpointName, WorkbenchCheckpointReceipt, WorkbenchCheckpointReferences,
};

fn selected_source(model: &mut AppModel, conversation: u8) -> peritus_app_protocol::WorkbenchQuery {
    let source = peritus_app_protocol::WorkbenchQuery::new(
        peritus_app_protocol::ConversationId::new([conversation; 16]).unwrap(),
        WorkspaceId::new([4; 16]).unwrap(),
    );
    model.chat.workbench.selected = Some(source);
    model.chat.workbench.snapshot = Some(
        WorkbenchSnapshot::new(
            source,
            4,
            ConversationTitle::new("Source".to_owned()).unwrap(),
            false,
            false,
        )
        .unwrap(),
    );
    source
}

fn checkpoint_receipt(
    source: peritus_app_protocol::WorkbenchQuery,
    checkpoint: u8,
    name: &str,
    goal_revision: Option<u64>,
) -> WorkbenchCheckpointReceipt {
    WorkbenchCheckpointReceipt::new(
        peritus_app_protocol::ControlOperationId::new([checkpoint; 16]).unwrap(),
        source,
        3,
        WorkbenchCheckpointName::new(name.to_owned()).unwrap(),
        WorkbenchCheckpointReferences::new(2, 3, 4, goal_revision),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )
    .unwrap()
}

#[test]
fn fork_command_submits_exact_checkpoint_and_opens_the_non_running_child() {
    let mut model = enabled_model();
    model.features.push(
        ProtocolFeatureName::well_known(WellKnownProtocolFeature::ConversationForks).unwrap(),
    );
    let source = selected_source(&mut model, 63);
    let checkpoint = peritus_app_protocol::ControlOperationId::new([0x40; 16]).unwrap();
    model.chat.workbench.checkpoint_receipt =
        Some(checkpoint_receipt(source, 0x40, "Historical branch point", Some(5)));
    model.chat.buffer =
        format!("/fork {} read-only time=60000 requests=3 tools=8 tokens=20000", "40".repeat(16));

    let sent = request(&enter_with_metadata(&mut model));
    let AppRequestPayload::WorkbenchCommand(command) = sent.payload() else { panic!("fork") };
    let peritus_app_protocol::WorkbenchIntent::ForkConversation(fork) = command.intent() else {
        panic!("fork intent")
    };
    assert_eq!(command.expected_revision(), 4);
    assert_eq!(fork.source_revision(), 2);
    assert_eq!(fork.context_generation(), 3);
    assert_eq!(fork.brief_revision(), 4);
    assert_eq!(fork.goal_revision(), 5);
    assert_eq!(fork.checkpoint(), checkpoint);
    assert_eq!(
        fork.allocation(),
        Some(peritus_app_protocol::WorkbenchForkBudget::new(60_000, 3, 8, 20_000).unwrap())
    );
    assert_ne!(fork.child().conversation(), source.conversation());
    let child = fork.child();
    let refresh = request(&respond(&mut model, &sent, receipt(command)));
    assert!(
        matches!(refresh.payload(), AppRequestPayload::QueryWorkbench(query) if *query == child)
    );
    assert_eq!(model.chat.workbench.selected, Some(child));
}

#[test]
fn fork_requires_the_exact_inspected_checkpoint_and_retains_the_draft() {
    let mut model = enabled_model();
    model.features.push(
        ProtocolFeatureName::well_known(WellKnownProtocolFeature::ConversationForks).unwrap(),
    );
    let source = selected_source(&mut model, 64);
    model.chat.workbench.checkpoint_receipt =
        Some(checkpoint_receipt(source, 0x41, "Another checkpoint", Some(5)));
    let draft = format!("/fork {} read-only", "40".repeat(16));
    model.chat.buffer.clone_from(&draft);

    assert!(enter_with_metadata(&mut model).is_empty());
    assert_eq!(model.chat.buffer, draft);
    assert!(
        model
            .notice
            .as_ref()
            .is_some_and(|notice| notice.text.contains("/checkpoint show <checkpoint-id>"))
    );
}

#[test]
fn read_only_fork_without_a_governing_goal_has_no_allocation() {
    let mut model = enabled_model();
    model.features.push(
        ProtocolFeatureName::well_known(WellKnownProtocolFeature::ConversationForks).unwrap(),
    );
    let source = selected_source(&mut model, 65);
    model.chat.workbench.checkpoint_receipt =
        Some(checkpoint_receipt(source, 0x42, "Ungoverned branch point", None));
    model.chat.buffer = format!("/fork {} read-only", "42".repeat(16));

    let sent = request(&enter_with_metadata(&mut model));
    let AppRequestPayload::WorkbenchCommand(command) = sent.payload() else { panic!("fork") };
    let peritus_app_protocol::WorkbenchIntent::ForkConversation(fork) = command.intent() else {
        panic!("fork intent")
    };
    assert_eq!(fork.goal_revision(), 0);
    assert_eq!(fork.allocation(), None);
}

#[test]
fn governed_read_only_fork_requires_an_allocation_and_retains_the_draft() {
    let mut model = enabled_model();
    model.features.push(
        ProtocolFeatureName::well_known(WellKnownProtocolFeature::ConversationForks).unwrap(),
    );
    let source = selected_source(&mut model, 66);
    model.chat.workbench.checkpoint_receipt =
        Some(checkpoint_receipt(source, 0x43, "Governed branch point", Some(2)));
    let draft = format!("/fork {} read-only", "43".repeat(16));
    model.chat.buffer.clone_from(&draft);

    assert!(enter_with_metadata(&mut model).is_empty());
    assert_eq!(model.chat.buffer, draft);
    assert!(
        model
            .notice
            .as_ref()
            .is_some_and(|notice| notice.text.contains("requires all four allocation fields"))
    );
}

#[test]
fn fork_of_a_maximum_length_unicode_title_submits_instead_of_silently_stalling() {
    let mut model = enabled_model();
    model.features.push(
        ProtocolFeatureName::well_known(WellKnownProtocolFeature::ConversationForks).unwrap(),
    );
    let source = selected_source(&mut model, 67);
    let title = format!("{}x", "界".repeat(85));
    assert_eq!(title.len(), peritus_app_protocol::MAX_CONVERSATION_TITLE_BYTES);
    model.chat.workbench.snapshot = Some(
        WorkbenchSnapshot::new(source, 4, ConversationTitle::new(title).unwrap(), false, false)
            .unwrap(),
    );
    model.chat.workbench.checkpoint_receipt =
        Some(checkpoint_receipt(source, 0x44, "Long title", None));
    model.chat.buffer = format!("/fork {} read-only", "44".repeat(16));
    let effects = enter_with_metadata(&mut model);
    let sent = request(&effects);
    let AppRequestPayload::WorkbenchCommand(command) = sent.payload() else { panic!("fork") };
    let peritus_app_protocol::WorkbenchIntent::ForkConversation(fork) = command.intent() else {
        panic!("fork intent")
    };
    assert!(fork.title().as_str().starts_with("Fork of 界"));
    assert!(fork.title().as_str().len() <= peritus_app_protocol::MAX_CONVERSATION_TITLE_BYTES);
}

#[test]
fn isolated_fork_does_not_open_the_child_under_the_parent_workspace_context() {
    let mut model = enabled_model();
    model.features.push(
        ProtocolFeatureName::well_known(WellKnownProtocolFeature::ConversationForks).unwrap(),
    );
    let source = selected_source(&mut model, 68);
    model.chat.workbench.checkpoint_receipt =
        Some(checkpoint_receipt(source, 0x45, "Isolated", Some(2)));
    model.chat.buffer = format!(
        "/fork {} isolated {} time=60000 requests=3 tools=8 tokens=20000",
        "45".repeat(16),
        "46".repeat(16)
    );
    let sent = request(&enter_with_metadata(&mut model));
    let AppRequestPayload::WorkbenchCommand(command) = sent.payload() else { panic!("fork") };
    let peritus_app_protocol::WorkbenchIntent::ForkConversation(fork) = command.intent() else {
        panic!("fork intent")
    };
    let effects = respond(&mut model, &sent, receipt(command));
    assert_eq!(model.chat.workbench.selected, Some(source));
    assert!(
        matches!(effects.as_slice(), [Effect::OpenConversation(query)] if *query == fork.child())
    );
    assert!(model.chat.workbench.unresolved.is_none());
    assert!(model.chat.buffer.is_empty());

    // A second draft typed while the fork was pending must not disappear into a new workspace.
    let mut model = enabled_model();
    model.features.push(
        ProtocolFeatureName::well_known(WellKnownProtocolFeature::ConversationForks).unwrap(),
    );
    let source = selected_source(&mut model, 69);
    model.chat.workbench.checkpoint_receipt =
        Some(checkpoint_receipt(source, 0x45, "Isolated", Some(2)));
    model.chat.buffer = format!(
        "/fork {} isolated {} time=60000 requests=3 tools=8 tokens=20000",
        "45".repeat(16),
        "46".repeat(16)
    );
    let sent = request(&enter_with_metadata(&mut model));
    let AppRequestPayload::WorkbenchCommand(command) = sent.payload() else { panic!("fork") };
    model.chat.buffer = "Actually, keep working here first".into();
    let effects = respond(&mut model, &sent, receipt(command));
    assert!(effects.iter().all(|effect| !matches!(effect, Effect::OpenConversation(_))));
    assert_eq!(model.chat.workbench.selected, Some(source));
    assert_eq!(model.chat.buffer, "Actually, keep working here first");
    assert!(model.chat.workbench.message.contains("/sessions open"));
    assert!(model.chat.workbench.message.contains("preserve your new draft"));
}

#[test]
fn explicit_child_open_requests_launcher_scope_and_rejects_malformed_workspace_ids() {
    let mut model = enabled_model();
    let source = selected_source(&mut model, 70);
    model.chat.buffer = format!("/sessions open {} {}", "46".repeat(16), "47".repeat(16));
    let effects = key(&mut model, KeyCode::Enter);
    assert!(matches!(effects.as_slice(), [Effect::OpenConversation(query)]
        if query.conversation().as_bytes() == &[0x46; 16] && query.workspace().as_bytes() == &[0x47; 16]));
    assert_eq!(model.chat.workbench.selected, Some(source));
    let draft = format!("/sessions open {} invalid", "46".repeat(16));
    model.chat.buffer.clone_from(&draft);
    assert!(key(&mut model, KeyCode::Enter).is_empty());
    assert_eq!(model.chat.buffer, draft);
    assert_eq!(model.chat.workbench.selected, Some(source));
}

#[test]
fn reopened_child_accepts_metadata_changes_with_its_own_workspace_binding() {
    let mut model = enabled_model();
    let child = peritus_app_protocol::WorkbenchQuery::new(
        peritus_app_protocol::ConversationId::new([0x48; 16]).unwrap(),
        WorkspaceId::new([0x49; 16]).unwrap(),
    );
    let launch = ProductLaunchContext::new(
        child.workspace(),
        "registered child root".into(),
        Vec::new(),
        None,
    )
    .unwrap()
    .with_conversation(child)
    .unwrap();
    model.product.as_mut().unwrap().launch = launch;
    model.chat.workbench.selected = Some(child);
    model.chat.workbench.snapshot = Some(
        WorkbenchSnapshot::new(
            child,
            1,
            ConversationTitle::new("Fork of source".into()).unwrap(),
            false,
            false,
        )
        .unwrap(),
    );
    model.chat.buffer = "/sessions rename Changed my mind about the title".into();
    let sent = request(&enter_with_metadata(&mut model));
    let AppRequestPayload::WorkbenchCommand(command) = sent.payload() else { panic!("rename") };
    assert_eq!(command.query(), child);
    assert_eq!(command.expected_revision(), 1);
    assert!(
        matches!(command.intent(), peritus_app_protocol::WorkbenchIntent::RenameConversation(title)
        if title.as_str() == "Changed my mind about the title")
    );
    respond(&mut model, &sent, receipt(command));
    assert!(model.chat.buffer.is_empty());
    assert_eq!(model.chat.workbench.selected, Some(child));
}
