use super::*;
use peritus_app_protocol::{
    AppRequestEnvelope, AppResponseEnvelope, AppResponsePayload, ConversationTitle,
    ProtocolFeatureName, WellKnownProtocolFeature, WorkbenchCommand, WorkbenchReceipt,
    WorkbenchSnapshot,
};

mod brief;
mod checkpoints;
mod compaction;
mod context;
mod conversation;
mod files;
mod fork;
mod goal;
mod images;
mod init;
mod library;
mod memory;
mod navigation;
mod onboarding;
mod permissions;
mod preview;
mod queue;
mod rejection;
mod review;

fn enabled_model() -> AppModel {
    let mut model = model();
    model.features = vec![
        ProtocolFeatureName::well_known(WellKnownProtocolFeature::WorkbenchControl)
            .expect("feature"),
    ];
    model
}

fn active_chat_writer_binding(model: &mut AppModel, provider: ProviderProfileId, model_id: &str) {
    let run = model.chat.run_id.unwrap_or_else(|| RunId::new([0x91; 16]).expect("run"));
    model.chat.run_id = Some(run);
    let writer = peritus_app_protocol::ProductModelChoice::new(model_id.to_owned(), true)
        .expect("selected writer model");
    let models = ProductRoleModels::new(
        writer,
        model.chat.models.reviewer().clone(),
        model.chat.models.fixer().clone(),
    );
    let providers =
        peritus_app_protocol::ProductProviderSelection::new(provider, provider, provider);
    let workspace = model.product.as_ref().expect("product").launch.workspace_id();
    let snapshot = peritus_app_protocol::ProductRunSnapshot::new(
        run,
        workspace,
        providers,
        peritus_app_protocol::ProductRunPhase::Writing,
        1,
        "active chat".to_owned(),
        "working".to_owned(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        crate::test_support::run_operation(run, peritus_app_protocol::ProductRunPhase::Writing),
    )
    .expect("run snapshot");
    let binding = ProductInteractionSnapshot::new(
        snapshot,
        ProductInteractionMode::Chat,
        models,
        1,
        1,
        Vec::new(),
        None,
    )
    .expect("active chat binding");
    model.accept_chat(binding);
}
fn request(effects: &[Effect]) -> AppRequestEnvelope {
    let [Effect::Send(AppMessage::Request(request))] = effects else {
        panic!("one typed request: {effects:?}")
    };
    request.clone()
}
fn respond(
    model: &mut AppModel,
    request: &AppRequestEnvelope,
    payload: AppResponsePayload,
) -> Vec<Effect> {
    model.update(Action::Message(AppMessage::Response(AppResponseEnvelope::new(
        request.context(),
        request.request_id(),
        request.correlation_id(),
        payload,
    ))))
}
fn enter_with_metadata(model: &mut AppModel) -> Vec<Effect> {
    let snapshot = model.chat.workbench.snapshot.clone().expect("selected metadata");
    let effects = key(model, KeyCode::Enter);
    if model.chat.workbench.snapshot_refresh_command.is_some()
        && matches!(effects.as_slice(), [Effect::Send(AppMessage::Request(sent))]
        if matches!(sent.payload(), AppRequestPayload::QueryWorkbench(_)))
    {
        let sent = request(&effects);
        return respond(model, &sent, AppResponsePayload::Workbench(snapshot));
    }
    effects
}

fn create(model: &mut AppModel) -> (AppRequestEnvelope, WorkbenchCommand) {
    model.chat.buffer = "/sessions new Private fixture".to_owned();
    let request = request(&key(model, KeyCode::Enter));
    let AppRequestPayload::WorkbenchCommand(command) = request.payload() else { panic!("command") };
    (request.clone(), command.clone())
}
fn receipt(command: &WorkbenchCommand) -> AppResponsePayload {
    AppResponsePayload::WorkbenchReceipt(
        WorkbenchReceipt::new(
            command.operation(),
            command.query(),
            command.expected_revision() + 1,
            peritus_types::Sha256Digest::new([51; 32]),
        )
        .expect("receipt"),
    )
}

#[test]
fn new_conversation_needs_receipt_then_detaches_the_previous_chat_without_cancelling_it() {
    let mut unsupported = model();
    unsupported.chat.buffer = "/sessions new title".to_owned();
    assert!(key(&mut unsupported, KeyCode::Enter).is_empty());
    assert_eq!(unsupported.chat.buffer, "/sessions new title");
    let mut model = enabled_model();
    let active = RunId::new([53; 16]).expect("run");
    model.chat.run_id = Some(active);
    let (sent, command) = create(&mut model);
    assert!(model.chat.workbench.snapshot.is_none());
    assert!(!model.chat.buffer.is_empty(), "sending is not durable acceptance");
    let query = request(&respond(&mut model, &sent, receipt(&command)));
    assert!(model.chat.buffer.is_empty());
    let snapshot = WorkbenchSnapshot::new(
        command.query(),
        1,
        ConversationTitle::new("Private fixture".to_owned()).expect("title"),
        false,
        false,
    )
    .expect("snapshot");
    respond(&mut model, &query, AppResponsePayload::Workbench(snapshot));
    assert_eq!(model.chat.workbench.snapshot.as_ref().expect("metadata").revision(), 1);
    assert!(model.chat.run_id.is_none());
    key(&mut model, KeyCode::Esc);
    model.chat.buffer = "/sessions pin".to_owned();
    let pin = request(&enter_with_metadata(&mut model));
    assert!(
        matches!(pin.payload(), AppRequestPayload::WorkbenchCommand(command) if command.expected_revision() == 1 && matches!(command.intent(), peritus_app_protocol::WorkbenchIntent::PinConversation(true)))
    );
}

#[test]
fn lost_response_is_resolved_after_reconnect_without_reissuing_a_mutation() {
    let mut model = enabled_model();
    let (sent, command) = create(&mut model);
    model.update(Action::Disconnected("lost response".to_owned()));
    key(&mut model, KeyCode::Esc);
    model.chat.buffer = "new draft stays here".to_owned();
    model.chat.cursor = 4;
    model.update(Action::Connected {
        context: sent.context(),
        limits: AppProtocolLimits::PRODUCTION,
        server: "reconnected".to_owned(),
        downgraded: false,
    });
    let effects = model.update(Action::NegotiatedFeatures {
        context: sent.context(),
        features: vec![
            ProtocolFeatureName::well_known(WellKnownProtocolFeature::WorkbenchControl)
                .expect("feature"),
        ],
    });
    let lookup = request(&effects);
    assert_eq!(lookup.payload(), &AppRequestPayload::QueryWorkbenchReceipt(command.clone()));
    assert_eq!(respond(&mut model, &lookup, receipt(&command)).len(), 1);
    assert_eq!(model.chat.buffer, "new draft stays here");
    assert_eq!(model.chat.cursor, 4);
    assert!(!model.chat.workbench.open, "receipt must not reopen a dismissed panel");
    assert!(model.chat.run_id.is_none());
}

#[test]
fn receipt_query_rejection_does_not_prove_the_original_mutation_was_rejected() {
    let mut model = enabled_model();
    let (sent, command) = create(&mut model);
    model.update(Action::Disconnected("lost response".to_owned()));
    model.update(Action::Connected {
        context: sent.context(),
        limits: AppProtocolLimits::PRODUCTION,
        server: "reconnected".to_owned(),
        downgraded: false,
    });
    let lookup = request(&model.update(Action::NegotiatedFeatures {
        context: sent.context(),
        features: vec![ProtocolFeatureName::well_known(
            WellKnownProtocolFeature::WorkbenchControl).expect("feature")],
    }));
    respond(
        &mut model,
        &lookup,
        AppResponsePayload::Error(peritus_app_protocol::AppProtocolError::new(
            peritus_app_protocol::AppErrorCode::MissingRequiredFeature,
            None,
        )),
    );
    key(&mut model, KeyCode::Esc);
    model.chat.buffer = "/sessions new must not replace unknown intent".to_owned();
    assert!(key(&mut model, KeyCode::Enter).is_empty());
    model.chat.buffer = "/sessions retry".to_owned();
    let retry = request(&key(&mut model, KeyCode::Enter));
    assert_eq!(retry.payload(), &AppRequestPayload::WorkbenchCommand(command));
}

#[test]
fn absent_receipt_can_only_retry_the_exact_operation_and_stale_rejection_requires_refresh() {
    let mut model = enabled_model();
    let (sent, command) = create(&mut model);
    let failure =
        |code| AppResponsePayload::Error(peritus_app_protocol::AppProtocolError::new(code, None));
    respond(&mut model, &sent, failure(peritus_app_protocol::AppErrorCode::Backpressure));
    key(&mut model, KeyCode::Esc);
    model.chat.buffer = "/sessions retry".to_owned();
    let retry = request(&key(&mut model, KeyCode::Enter));
    assert_eq!(retry.payload(), &AppRequestPayload::WorkbenchCommand(command));
    respond(&mut model, &retry, failure(peritus_app_protocol::AppErrorCode::StaleRevision));
    model.chat.buffer = "/sessions pin".to_owned();
    assert!(key(&mut model, KeyCode::Enter).is_empty());
    assert_eq!(model.chat.buffer, "/sessions pin");
}

#[test]
fn metadata_panel_keeps_composer_and_return_hint_at_all_supported_sizes() {
    use ratatui::{Terminal, backend::TestBackend};
    let mut model = enabled_model();
    model.chat.buffer = "retained draft λ".to_owned();
    model.chat.cursor = 3;
    model.chat.scroll = 7;
    model.slash_command("/sessions");
    for (width, height) in [(40, 12), (80, 24), (120, 38)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
        let frame = terminal.draw(|frame| crate::render::draw(frame, &model)).expect("draw");
        let text: String =
            frame.buffer.content().iter().map(ratatui::buffer::Cell::symbol).collect();
        for expected in ["Sessions", "Esc back", "Message Peritus", "retained draft λ"] {
            assert!(text.contains(expected), "{width}x{height} missing {expected}: {text}");
        }
    }
    key(&mut model, KeyCode::PageDown);
    key(&mut model, KeyCode::Esc);
    assert_eq!(model.chat.buffer, "retained draft λ");
    assert_eq!(model.chat.cursor, 3);
    assert_eq!(model.chat.scroll, 7);
}

#[test]
fn normal_slash_navigation_leaves_goal_for_each_other_workbench_panel() {
    use ratatui::{Terminal, backend::TestBackend};

    for (command, expected_panel) in [
        ("/brief", "Task brief"),
        ("/queue", "Queue · durable inputs"),
        ("/context", "Context · read only"),
        ("/permissions", "Permissions · enforced"),
        ("/memory", "Memory · project guidance"),
        ("/checkpoint navigation", "Checkpoint · safe rewind"),
    ] {
        let mut model = enabled_model();
        model.features.extend(
            [
                WellKnownProtocolFeature::WorkbenchBrief,
                WellKnownProtocolFeature::WorkbenchGoals,
                WellKnownProtocolFeature::WorkbenchInputs,
                WellKnownProtocolFeature::WorkbenchContext,
                WellKnownProtocolFeature::WorkbenchPermissions,
                WellKnownProtocolFeature::WorkbenchMemory,
                WellKnownProtocolFeature::WorkbenchCheckpoints,
            ]
            .into_iter()
            .map(|feature| ProtocolFeatureName::well_known(feature).expect("feature")),
        );
        let query = peritus_app_protocol::WorkbenchQuery::new(
            peritus_app_protocol::ConversationId::new([59; 16]).expect("conversation"),
            model.product.as_ref().expect("product").launch.workspace_id(),
        );
        model.chat.workbench.selected = Some(query);
        model.chat.workbench.snapshot = Some(
            WorkbenchSnapshot::new(
                query,
                7,
                ConversationTitle::new("Navigation fixture".to_owned()).expect("title"),
                false,
                false,
            )
            .expect("snapshot"),
        );

        model.chat.buffer = "/goal retain this objective".to_owned();
        model.chat.cursor = model.chat.buffer.len();
        let goal_read = request(&key(&mut model, KeyCode::Enter));
        assert!(matches!(goal_read.payload(), AppRequestPayload::QueryWorkbenchBrief(_)));
        respond(
            &mut model,
            &goal_read,
            AppResponsePayload::Error(peritus_app_protocol::AppProtocolError::new(
                peritus_app_protocol::AppErrorCode::Backpressure,
                None,
            )),
        );
        assert!(model.chat.workbench.goal_mode);
        assert_eq!(
            model.chat.workbench.goal_draft.as_ref().expect("goal draft").objective().as_str(),
            "retain this objective"
        );

        key(&mut model, KeyCode::Esc);
        model.chat.buffer = command.to_owned();
        model.chat.cursor = model.chat.buffer.len();
        assert!(!enter_with_metadata(&mut model).is_empty(), "{command} did not navigate");
        assert!(!model.chat.workbench.goal_mode, "{command} left the goal panel sticky");
        assert_eq!(model.chat.buffer, command, "{command} composer was not retained");
        assert_eq!(
            model
                .chat
                .workbench
                .goal_draft
                .as_ref()
                .expect("retained goal draft")
                .objective()
                .as_str(),
            "retain this objective",
            "{command} discarded the goal draft"
        );

        let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("terminal");
        let frame = terminal.draw(|frame| crate::render::draw(frame, &model)).expect("draw");
        let text: String =
            frame.buffer.content().iter().map(ratatui::buffer::Cell::symbol).collect();
        assert!(text.contains(expected_panel), "{command} rendered the wrong panel: {text}");
        assert!(!text.contains("Goal · durable execution control"), "{command} rendered goal");
    }
}

#[test]
fn sessions_literal_query_accepts_exact_source_linked_library_page() {
    use peritus_app_protocol::{
        ConversationLibraryItem, ConversationLibraryPage, ConversationMessageSource,
        ConversationSearchSnippet, ConversationSearchText, WorkbenchExecutionState,
    };
    let mut model = enabled_model();
    model.features.push(
        ProtocolFeatureName::well_known(WellKnownProtocolFeature::ConversationLibrary).unwrap(),
    );
    for feature in [
        WellKnownProtocolFeature::WorkbenchInputs,
        WellKnownProtocolFeature::WorkbenchExecution,
        WellKnownProtocolFeature::WorkbenchConversation,
    ] {
        model.features.push(ProtocolFeatureName::well_known(feature).unwrap());
    }
    model.chat.buffer = "/sessions ancient phrase".to_owned();
    let sent = request(&key(&mut model, KeyCode::Enter));
    let AppRequestPayload::QueryConversationLibrary(query) = sent.payload() else {
        panic!("library query")
    };
    assert_eq!(query.literal().map(ConversationSearchText::as_str), Some("ancient phrase"));
    let scope = peritus_app_protocol::WorkbenchQuery::new(
        peritus_app_protocol::ConversationId::new([61; 16]).unwrap(),
        query.workspace(),
    );
    let snippet = ConversationSearchSnippet::new(
        ConversationMessageSource::Input {
            conversation: scope.conversation(),
            input: peritus_app_protocol::WorkbenchInputId::new([62; 16]).unwrap(),
            revision: 1,
        },
        "exact ancient phrase source".to_owned(),
    )
    .unwrap();
    let item = ConversationLibraryItem::new(
        scope,
        ConversationTitle::new("Older work".to_owned()).unwrap(),
        true,
        false,
        7,
        None,
        false,
        "preserved handoff".to_owned(),
        Some(snippet),
        None,
    )
    .unwrap();
    let page = ConversationLibraryPage::new(query.clone(), 1, None, vec![item]).unwrap();
    assert!(respond(&mut model, &sent, AppResponsePayload::ConversationLibrary(page)).is_empty());
    assert_eq!(model.chat.workbench.library.as_ref().unwrap().total(), 1);
    key(&mut model, KeyCode::Esc);
    model.chat.buffer =
        format!("/sessions open {}", crate::model::format_id(scope.conversation().as_bytes()));
    let opened = request(&key(&mut model, KeyCode::Enter));
    assert!(
        matches!(opened.payload(), AppRequestPayload::QueryWorkbenchExecution(query) if *query == scope)
    );
    assert_eq!(model.chat.workbench.selected, Some(scope));
    assert_eq!(
        model.chat.buffer,
        format!("/sessions open {}", crate::model::format_id(scope.conversation().as_bytes()))
    );
    let snapshot = WorkbenchSnapshot::new(
        scope,
        7,
        ConversationTitle::new("Older work".to_owned()).unwrap(),
        false,
        false,
    )
    .unwrap();
    assert!(
        respond(
            &mut model,
            &opened,
            AppResponsePayload::WorkbenchExecution(
                WorkbenchExecutionState::new(snapshot, None, false).unwrap(),
            ),
        )
        .is_empty()
    );
    assert!(model.chat.buffer.is_empty());
}
