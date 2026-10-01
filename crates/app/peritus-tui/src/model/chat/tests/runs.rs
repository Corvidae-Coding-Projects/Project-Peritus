//! Run dashboard navigation and mixed settlement observations.
use super::*;
use peritus_app_protocol::{
    ProductProviderSelection, ProductRunObservation, ProductRunPhase, ProductRunSnapshot,
    ProtocolFeatureName, WellKnownProtocolFeature,
};
use peritus_run_settlement::{SettlementCause, SettlementReducer};

#[test]
fn a_slow_run_lookup_does_not_trap_new_conversation_navigation_or_late_reply_routing() {
    use peritus_app_protocol::{
        AppResponseEnvelope, AppResponsePayload, ConversationId, ProductInteractionBinding,
        WorkbenchQuery,
    };
    let mut model = durable_chat_model();
    let old_run = RunId::new([98; 16]).unwrap();
    let workspace = WorkspaceId::new([4; 16]).unwrap();
    model.chat.run_id = Some(old_run);
    model.features.push(
        ProtocolFeatureName::well_known(WellKnownProtocolFeature::WorkbenchRunBinding).unwrap(),
    );
    model.chat.buffer = "pending draft".into();
    let effects = key(&mut model, KeyCode::Enter);
    let [Effect::Send(AppMessage::Request(old_request))] = effects.as_slice() else {
        panic!("lookup")
    };
    model.chat.buffer = "/new".into();
    assert!(key(&mut model, KeyCode::Enter).is_empty());
    assert!(model.chat.run_id.is_none());
    assert!(!model.pending.contains_key(&old_request.request_id()));
    assert!(!model.pending_started.contains_key(&old_request.request_id()));
    model.chat.buffer = "Different task instead".into();
    let effects = key(&mut model, KeyCode::Enter);
    assert!(matches!(effects.as_slice(), [Effect::Send(AppMessage::Request(sent))]
        if matches!(sent.payload(), AppRequestPayload::WorkbenchCommand(command)
            if matches!(command.intent(), peritus_app_protocol::WorkbenchIntent::CreateConversation(_)))));
    let new_conversation = model.chat.workbench.selected;
    let snapshot = ProductRunSnapshot::new(
        old_run,
        workspace,
        model.chat_providers().unwrap(),
        ProductRunPhase::Complete,
        1,
        "Old task".into(),
        "Ready".into(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
    )
    .unwrap();
    let binding = ProductInteractionBinding::new(
        ProductInteractionSnapshot::new(
            snapshot,
            ProductInteractionMode::Chat,
            ProductRoleModels::default(),
            1,
            1,
            Vec::new(),
            None,
        )
        .unwrap(),
        WorkbenchQuery::new(ConversationId::new([99; 16]).unwrap(), workspace),
    )
    .unwrap();
    model.update(Action::Message(AppMessage::Response(AppResponseEnvelope::new(
        old_request.context(),
        old_request.request_id(),
        old_request.correlation_id(),
        AppResponsePayload::InteractionBinding(binding),
    ))));
    assert!(model.chat.run_id.is_none());
    assert_eq!(model.chat.workbench.selected, new_conversation);
}

#[test]
fn delayed_binding_cannot_retarget_a_new_selection_or_steal_focus_from_an_inspector() {
    use peritus_app_protocol::{ConversationId, ProductInteractionBinding, WorkbenchQuery};
    let mut model = model();
    let run = RunId::new([95; 16]).unwrap();
    let workspace = WorkspaceId::new([4; 16]).unwrap();
    let query = WorkbenchQuery::new(ConversationId::new([96; 16]).unwrap(), workspace);
    let snapshot = ProductRunSnapshot::new(
        run,
        workspace,
        model.chat_providers().unwrap(),
        ProductRunPhase::Complete,
        1,
        "Read code".into(),
        "Ready".into(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
    )
    .unwrap();
    let binding = ProductInteractionBinding::new(
        ProductInteractionSnapshot::new(
            snapshot,
            ProductInteractionMode::Chat,
            ProductRoleModels::default(),
            1,
            1,
            Vec::new(),
            None,
        )
        .unwrap(),
        query,
    )
    .unwrap();
    let pending = PendingRequest::ChatBinding { run_id: run, opening: false };
    model.chat.run_id = Some(RunId::new([97; 16]).unwrap());
    assert!(model.accept_chat_binding(&binding, Some(&pending)).is_empty());
    assert!(model.chat.workbench.selected.is_none());
    assert_ne!(model.chat.run_id, Some(run));
    model.chat.run_id = Some(run);
    model.view = View::Runs;
    model.chat.workbench.open = true;
    assert!(model.accept_chat_binding(&binding, Some(&pending)).is_empty());
    assert_eq!(model.chat.workbench.selected, Some(query));
    assert_eq!(model.view, View::Runs);
    assert!(model.chat.workbench.open);
}

#[test]
fn cross_workspace_run_switch_retains_drafts_and_hands_off_only_the_selected_identity() {
    let mut model = model();
    let run = RunId::new([90; 16]).unwrap();
    let workspace = WorkspaceId::new([91; 16]).unwrap();
    let snapshot = ProductRunSnapshot::new(
        run,
        workspace,
        model.chat_providers().unwrap(),
        ProductRunPhase::Complete,
        1,
        "Other project".into(),
        "Ready".into(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
    )
    .unwrap();
    model.accept_product_runs(vec![snapshot]);
    let previous = RunId::new([92; 16]).unwrap();
    model.chat.run_id = Some(previous);
    model.chat.buffer = "An unfinished idea for this project".into();
    assert!(model.open_selected_conversation().is_empty());
    assert_eq!(model.chat.run_id, Some(previous));
    assert_eq!(model.chat.buffer, "An unfinished idea for this project");
    model.chat.buffer.clear();
    let effects = model.open_selected_conversation();
    assert!(matches!(effects.as_slice(), [Effect::OpenRun { run: actual, workspace: scope }]
        if *actual == run && *scope == workspace));
    assert_eq!(model.chat.run_id, Some(previous), "retain parent until launcher accepts");
}

#[test]
fn initial_run_lookup_failure_retains_the_destination_and_draft_for_retry() {
    use peritus_app_protocol::{
        AppErrorCode, AppProtocolError, AppResponseEnvelope, AppResponsePayload,
    };
    for code in [
        AppErrorCode::Internal,
        AppErrorCode::InvalidIdentifier,
        AppErrorCode::SessionMismatch,
        AppErrorCode::IdempotencyConflict,
    ] {
        let mut model = model();
        let run = RunId::new([93; 16]).unwrap();
        model.chat.run_id = Some(run);
        model.features.push(
            ProtocolFeatureName::well_known(WellKnownProtocolFeature::WorkbenchRunBinding).unwrap(),
        );
        model.chat.buffer = "Actually make that blue".into();
        let effects = key(&mut model, KeyCode::Enter);
        let [Effect::Send(AppMessage::Request(request))] = effects.as_slice() else {
            panic!("binding")
        };
        assert!(
            matches!(request.payload(), AppRequestPayload::QueryInteractionBinding(query) if query.run_id() == run)
        );
        model.update(Action::Message(AppMessage::Response(AppResponseEnvelope::new(
            request.context(),
            request.request_id(),
            request.correlation_id(),
            AppResponsePayload::Error(AppProtocolError::new(code, None)),
        ))));
        assert_eq!(model.chat.run_id, Some(run));
        assert_eq!(model.chat.buffer, "Actually make that blue");
        assert!(model.editor.is_none());
        let effects = key(&mut model, KeyCode::Enter);
        assert!(matches!(effects.as_slice(), [Effect::Send(AppMessage::Request(request))]
            if matches!(request.payload(), AppRequestPayload::QueryInteractionBinding(query) if query.run_id() == run)));
    }
}

#[test]
fn run_binding_routes_the_next_message_to_the_observed_conversation_without_starting_on_open() {
    use peritus_app_protocol::{
        AppResponseEnvelope, AppResponsePayload, ConversationId, ProductInteractionBinding,
        WorkbenchQuery,
    };
    let mut model = durable_chat_model();
    model.features.push(
        ProtocolFeatureName::well_known(WellKnownProtocolFeature::WorkbenchRunBinding).unwrap(),
    );
    let run = RunId::new([83; 16]).unwrap();
    let query = WorkbenchQuery::new(
        ConversationId::new([84; 16]).unwrap(),
        WorkspaceId::new([4; 16]).unwrap(),
    );
    let snapshot = ProductRunSnapshot::new(
        run,
        query.workspace(),
        model.chat_providers().unwrap(),
        ProductRunPhase::Complete,
        1,
        "Another run".into(),
        "Ready".into(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
    )
    .unwrap();
    model.accept_product_runs(vec![snapshot.clone()]);
    let effects = model.open_selected_conversation();
    let [Effect::Send(AppMessage::Request(request))] = effects.as_slice() else { panic!("lookup") };
    assert!(
        matches!(request.payload(), AppRequestPayload::QueryInteractionBinding(actual) if actual.run_id() == run)
    );
    model.chat.buffer = "Actually, use Celsius instead".into();
    assert!(key(&mut model, KeyCode::Enter).is_empty(), "input waits for the exact destination");
    let interaction = ProductInteractionSnapshot::new(
        snapshot,
        ProductInteractionMode::Chat,
        ProductRoleModels::default(),
        1,
        1,
        Vec::new(),
        None,
    )
    .unwrap();
    let effects = model.update(Action::Message(AppMessage::Response(AppResponseEnvelope::new(
        request.context(),
        request.request_id(),
        request.correlation_id(),
        AppResponsePayload::InteractionBinding(
            ProductInteractionBinding::new(interaction, query).unwrap(),
        ),
    ))));
    assert!(effects.is_empty(), "opening must not start inference");
    assert_eq!(model.chat.workbench.selected, Some(query));
    assert_eq!(model.chat.buffer, "Actually, use Celsius instead");
    let effects = key(&mut model, KeyCode::Enter);
    assert!(matches!(effects.as_slice(), [Effect::Send(AppMessage::Request(request))]
        if matches!(request.payload(), AppRequestPayload::QueryWorkbenchExecution(actual) if *actual == query)));
}

#[test]
fn opening_a_different_run_never_leaves_the_previous_conversation_as_the_input_target() {
    let mut model = model();
    let old = peritus_app_protocol::WorkbenchQuery::new(
        peritus_app_protocol::ConversationId::new([80; 16]).unwrap(),
        WorkspaceId::new([4; 16]).unwrap(),
    );
    model.chat.workbench.selected = Some(old);
    model.chat.workbench.open = true;
    model.chat.run_id = Some(RunId::new([81; 16]).unwrap());
    let run = RunId::new([82; 16]).unwrap();
    let providers = model.chat_providers().unwrap();
    model.accept_product_runs(vec![
        ProductRunSnapshot::new(
            run,
            old.workspace(),
            providers,
            ProductRunPhase::Complete,
            1,
            "Another run".into(),
            "Ready".into(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        )
        .unwrap(),
    ]);
    let effects = model.open_selected_conversation();
    assert!(!effects.is_empty());
    assert_ne!(
        model.chat.workbench.selected,
        Some(old),
        "input must not target the old session after opening another run"
    );
    assert!(!model.chat.workbench.open, "old metadata must not cover the newly opened transcript");
}

#[test]
fn runs_selects_current_conversation_and_refresh_clears_only_active_settlements() {
    let mut model = model();
    let provider = model.chat_providers().expect("providers").writer();
    let snapshots = [31, 32].map(|id| {
        ProductRunSnapshot::new(
            RunId::new([id; 16]).expect("run"),
            WorkspaceId::new([4; 16]).expect("workspace"),
            ProductProviderSelection::new(provider, provider, provider),
            ProductRunPhase::Writing,
            1,
            "task".to_owned(),
            "Working".to_owned(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        )
        .expect("snapshot")
    });
    model.accept_product_runs(snapshots.to_vec());
    model.chat.run_id = Some(snapshots[1].run_id());
    model.paste_chat("/runs");
    let _ = key(&mut model, KeyCode::Enter);
    assert_eq!(
        model.product.as_ref().expect("product").selected_run().expect("selected").run_id(),
        snapshots[1].run_id()
    );
    let settlement =
        SettlementReducer::new().settle(SettlementCause::Provider).expect("settlement");
    model.product.as_mut().expect("product").settlements.insert(snapshots[0].run_id(), settlement);
    let observations = vec![
        ProductRunObservation::new(snapshots[0].clone(), None).expect("active"),
        ProductRunObservation::new(snapshots[1].clone(), Some(settlement)).expect("settled"),
    ];
    model.accept_observation_query(&observations, None);
    let product = model.product.as_ref().expect("product");
    assert!(!product.settlements.contains_key(&snapshots[0].run_id()));
    assert_eq!(product.settlements.get(&snapshots[1].run_id()), Some(&settlement));
    assert_eq!(product.selected, 1);
    model.pending.clear();
    let effects = model.poll_product_runs();
    assert!(effects.iter().any(|effect| matches!(effect, Effect::Send(AppMessage::Request(request)) if matches!(request.payload(), AppRequestPayload::QueryProductRunObservations(_)))));
}
