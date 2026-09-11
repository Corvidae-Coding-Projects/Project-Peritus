use super::*;
use peritus_app_protocol::{
    AppResponseEnvelope, AppResponsePayload, ProductRoleModels, ProductRunPhase, ProductRunSnapshot,
};

pub(super) fn active(model: &mut AppModel) -> ProductInteractionSnapshot {
    let run = RunId::new([0x41; 16]).expect("run");
    model.chat.run_id = Some(run);
    let snapshot = ProductRunSnapshot::new(
        run,
        WorkspaceId::new([4; 16]).expect("workspace"),
        model.chat_providers().expect("providers"),
        ProductRunPhase::Writing,
        1,
        "Tetris".to_owned(),
        "Working".to_owned(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
    )
    .expect("snapshot");
    let snapshot = ProductInteractionSnapshot::new(
        snapshot,
        ProductInteractionMode::Chat,
        ProductRoleModels::default(),
        1,
        1,
        Vec::new(),
        None,
    )
    .expect("interaction");
    model.accept_chat(snapshot.clone());
    snapshot
}

#[test]
fn active_model_selection_is_sent_and_only_confirmed_after_daemon_acknowledgement() {
    let mut model = model();
    let before = active(&mut model);
    let discovery = model.slash_command("/model writer");
    let [Effect::Send(AppMessage::Request(query))] = discovery.as_slice() else {
        panic!("catalog query")
    };
    let catalog = ProductModelCatalog::new(
        model.chat_providers().expect("providers").writer(),
        "configured".to_owned(),
        vec![
            peritus_app_protocol::ProductModelInfo::new(
                "gpt-6-astra".to_owned(),
                "Astra".to_owned(),
                Some(true),
            )
            .expect("model"),
        ],
        1,
        false,
        String::new(),
    )
    .expect("catalog");
    model.handle_message(AppMessage::Response(AppResponseEnvelope::new(
        query.context(),
        query.request_id(),
        query.correlation_id(),
        AppResponsePayload::Models(catalog),
    )));
    let effects = key(&mut model, KeyCode::Enter);
    let [Effect::Send(AppMessage::Request(request))] = effects.as_slice() else {
        panic!("model update must be sent")
    };
    let AppRequestPayload::UpdateModels(update) = request.payload() else {
        panic!("dedicated selection request")
    };
    assert_eq!(update.run_id(), before.snapshot().run_id());
    assert_eq!(update.models().writer().id(), "gpt-6-astra");
    assert!(!update.models().writer().manual());
    assert_eq!(model.chat.models, ProductRoleModels::default(), "no optimistic applied model");
    assert!(model.notice.as_ref().expect("notice").text.contains("waiting for daemon"));
    assert!(model.slash_command("/new").is_empty());
    assert_eq!(model.chat.run_id, Some(update.run_id()));
    model.paste_chat("keep this draft");
    assert!(
        key(&mut model, KeyCode::Enter).is_empty(),
        "input waits for selection acknowledgement"
    );
    let acknowledged = ProductInteractionSnapshot::new(
        before.snapshot().clone(),
        before.mode(),
        update.models().clone(),
        1,
        1,
        Vec::new(),
        None,
    )
    .expect("ack");
    model.handle_message(AppMessage::Response(AppResponseEnvelope::new(
        request.context(),
        request.request_id(),
        request.correlation_id(),
        AppResponsePayload::Interaction(acknowledged),
    )));
    assert_eq!(model.chat.models.writer().id(), "gpt-6-astra");
    assert!(model.notice.as_ref().expect("notice").text.contains("in-flight turn is unchanged"));
    let sent = key(&mut model, KeyCode::Enter);
    assert!(sent.iter().any(|effect| matches!(effect, Effect::Send(AppMessage::Request(request)) if matches!(request.payload(), AppRequestPayload::Interact(value) if value.models().writer().id() == "gpt-6-astra" && value.request().task() == "keep this draft"))));
}

#[test]
fn disconnected_selection_does_not_claim_success_or_replace_confirmed_model() {
    let mut model = model();
    active(&mut model);
    model.context = None;
    assert!(model.slash_command("/model writer manual gpt-6-astra").is_empty());
    assert_eq!(model.chat.models, ProductRoleModels::default());
    assert!(model.notice.as_ref().expect("notice").text.contains("not sent"));
}

#[test]
fn new_conversations_preserve_explicit_role_selections() {
    let mut model = model();
    model.slash_command("/model writer manual gpt-6-astra");
    model.slash_command("/new");
    assert_eq!(model.chat.models.writer().id(), "gpt-6-astra");
}

#[test]
fn rejected_selection_retains_the_prior_model_and_user_draft() {
    let mut model = model();
    active(&mut model);
    let effects = model.slash_command("/model writer manual unavailable");
    let [Effect::Send(AppMessage::Request(request))] = effects.as_slice() else { panic!("update") };
    model.paste_chat("keep this draft");
    model.handle_message(AppMessage::Response(AppResponseEnvelope::new(
        request.context(),
        request.request_id(),
        request.correlation_id(),
        AppResponsePayload::Error(peritus_app_protocol::AppProtocolError::new(
            peritus_app_protocol::AppErrorCode::UnknownTag,
            None,
        )),
    )));
    assert_eq!(model.chat.models, ProductRoleModels::default());
    assert_eq!(model.chat.buffer, "keep this draft");
    assert!(!model.chat_submission_pending());
    assert_eq!(model.notice.as_ref().expect("error").level, NoticeLevel::Error);
}

#[test]
fn authoritative_poll_restores_selection_and_stale_poll_cannot_revert_it() {
    let mut model = model();
    let before = active(&mut model);
    let selected = ProductRoleModels::new(
        peritus_app_protocol::ProductModelChoice::new("gpt-6-astra".to_owned(), true)
            .expect("model"),
        peritus_app_protocol::ProductModelChoice::default(),
        peritus_app_protocol::ProductModelChoice::default(),
    );
    let updated = ProductInteractionSnapshot::new(
        before.snapshot().clone(),
        before.mode(),
        selected.clone(),
        1,
        1,
        vec![
            peritus_app_protocol::ProductActivity::new(
                1,
                peritus_app_protocol::ProductActivityKind::Status,
                "Model selection saved".to_owned(),
                String::new(),
            )
            .expect("activity"),
        ],
        None,
    )
    .expect("updated");
    model.accept_chat(updated);
    assert_eq!(
        model.chat.models, selected,
        "lost acknowledgement is recovered from authoritative polling"
    );
    model.accept_chat(before);
    assert_eq!(model.chat.models, selected, "delayed pre-selection reply cannot undo it");
}
