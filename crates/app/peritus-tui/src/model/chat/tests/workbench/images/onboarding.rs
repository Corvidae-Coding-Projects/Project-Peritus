use super::*;

fn fresh() -> AppModel {
    let mut model = opened();
    model.select_workbench_conversation(None);
    model.chat.buffer = "/attach /explicit/reference.gif".to_owned();
    model
}

fn start(model: &mut AppModel) -> (AppRequestEnvelope, WorkbenchCommand) {
    let sent = request(&key(model, KeyCode::Enter));
    let AppRequestPayload::WorkbenchCommand(command) = sent.payload() else {
        panic!("create metadata only: {sent:?}")
    };
    assert!(matches!(command.intent(), WorkbenchIntent::CreateConversation(_)));
    assert_eq!(command.expected_revision(), 0);
    assert!(model.chat.run_id.is_none());
    (sent.clone(), command.clone())
}

fn metadata(command: &WorkbenchCommand) -> AppResponsePayload {
    AppResponsePayload::Workbench(
        WorkbenchSnapshot::new(
            command.query(),
            1,
            ConversationTitle::new("Image conversation".to_owned()).expect("title"),
            false,
            false,
        )
        .expect("snapshot"),
    )
}

#[test]
fn first_attachment_creates_metadata_then_previews_without_starting_execution() {
    let mut model = fresh();
    let (sent, command) = start(&mut model);
    assert!(key(&mut model, KeyCode::Enter).is_empty());
    let refresh = request(&respond(&mut model, &sent, receipt(&command)));
    assert_eq!(refresh.payload(), &AppRequestPayload::QueryWorkbench(command.query()));
    assert_eq!(model.chat.buffer, "/attach /explicit/reference.gif");
    let effects = respond(&mut model, &refresh, metadata(&command));
    assert!(matches!(effects.as_slice(), [Effect::ReadImage { path, .. }]
        if path.to_string_lossy() == "/explicit/reference.gif"));
    assert!(model.chat.workbench.images.preview.is_none());
    assert!(model.chat.run_id.is_none());
    assert!(model.chat.workbench.unresolved.is_none());
    assert!(key(&mut model, KeyCode::Char('c')).is_empty());
}

#[test]
fn abandoning_first_attachment_or_changing_draft_never_reads_after_late_creation() {
    for escape in [false, true] {
        let mut model = fresh();
        let (sent, command) = start(&mut model);
        if escape {
            key(&mut model, KeyCode::Esc);
        }
        model.chat.buffer = "changed my mind".to_owned();
        let refresh = request(&respond(&mut model, &sent, receipt(&command)));
        assert!(respond(&mut model, &refresh, metadata(&command)).is_empty());
        assert_eq!(model.chat.buffer, "changed my mind");
        assert!(!model.workbench_request_pending());
        assert!(model.chat.run_id.is_none());
    }
}

#[test]
fn escape_after_creation_abandons_the_snapshot_continuation() {
    let mut model = fresh();
    let (sent, command) = start(&mut model);
    let refresh = request(&respond(&mut model, &sent, receipt(&command)));
    key(&mut model, KeyCode::Esc);
    assert!(respond(&mut model, &refresh, metadata(&command)).is_empty());
    assert_eq!(model.chat.buffer, "/attach /explicit/reference.gif");
    assert!(!model.workbench_request_pending());
}

#[test]
fn lost_creation_receipt_is_reconciled_without_creating_another_conversation() {
    let mut model = fresh();
    let (sent, command) = start(&mut model);
    model.update(Action::Disconnected("creation receipt lost".to_owned()));
    model.update(Action::Connected {
        context: sent.context(),
        limits: AppProtocolLimits::PRODUCTION,
        server: "reconnected".to_owned(),
        downgraded: false,
    });
    let lookup = request(&model.update(Action::NegotiatedFeatures {
        context: sent.context(),
        features: vec![
            ProtocolFeatureName::well_known(WellKnownProtocolFeature::WorkbenchControl)
                .expect("control"),
            ProtocolFeatureName::well_known(WellKnownProtocolFeature::WorkbenchImages)
                .expect("image"),
        ],
    }));
    assert_eq!(lookup.payload(), &AppRequestPayload::QueryWorkbenchReceipt(command.clone()));
    let refresh = request(&respond(&mut model, &lookup, receipt(&command)));
    let effects = respond(&mut model, &refresh, metadata(&command));
    assert!(matches!(effects.as_slice(), [Effect::ReadImage { .. }]));
    assert_eq!(model.chat.workbench.selected, Some(command.query()));
    assert!(model.chat.run_id.is_none());
}

#[test]
fn rejected_creation_retains_draft_and_can_retry_with_a_new_operation() {
    let mut model = fresh();
    let (sent, command) = start(&mut model);
    respond(
        &mut model,
        &sent,
        AppResponsePayload::Error(peritus_app_protocol::AppProtocolError::new(
            peritus_app_protocol::AppErrorCode::StaleRevision,
            None,
        )),
    );
    assert!(model.chat.workbench.selected.is_none());
    assert!(model.chat.workbench.unresolved.is_none());
    assert_eq!(model.chat.buffer, "/attach /explicit/reference.gif");
    key(&mut model, KeyCode::Esc);
    let (_, retry) = start(&mut model);
    assert_ne!(command.operation(), retry.operation());
}

#[test]
fn invalid_path_never_creates_a_conversation() {
    let mut model = fresh();
    model.chat.buffer = format!("/attach /{}", "x".repeat(4096));
    assert!(key(&mut model, KeyCode::Enter).is_empty());
    assert!(model.chat.workbench.selected.is_none());
    assert!(model.chat.workbench.unresolved.is_none());
}

#[test]
fn selected_conversation_without_snapshot_refreshes_before_reading() {
    let mut model = opened();
    let snapshot = model.chat.workbench.snapshot.take().expect("metadata");
    model.chat.buffer = "/attach /explicit/reference.gif".to_owned();
    let refresh = request(&key(&mut model, KeyCode::Enter));
    assert_eq!(refresh.payload(), &AppRequestPayload::QueryWorkbench(snapshot.query()));
    let effects = respond(&mut model, &refresh, AppResponsePayload::Workbench(snapshot));
    assert!(matches!(effects.as_slice(), [Effect::ReadImage { .. }]));
    assert!(model.chat.run_id.is_none());
}
