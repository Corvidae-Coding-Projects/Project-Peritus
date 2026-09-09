use super::*;
use peritus_app_protocol::{
    AppResponseEnvelope, AppResponsePayload, ProductModelEffort as EffortLevel,
};

#[test]
fn every_effort_is_selectable_and_survives_model_change_and_new_conversation() {
    for role in ["writer", "reviewer", "fixer"] {
        for effort in EffortLevel::ALL {
            let mut model = model();
            assert!(model.slash_command(&format!("/effort {role}")).is_empty());
            assert!(model.chat.effort_picker());
            key(&mut model, KeyCode::Home);
            for _ in 0..effort.tag() {
                key(&mut model, KeyCode::Down);
            }
            assert!(key(&mut model, KeyCode::Enter).is_empty());
            assert_eq!(model.chat.model_role.choice(&model.chat.models).effort(), effort);
            assert!(!model.chat.effort_picker());
            model.slash_command(&format!("/model {role} manual arbitrary-model"));
            model.slash_command("/new");
            let choice = model.chat.model_role.choice(&model.chat.models);
            assert_eq!(choice.effort(), effort);
            assert_eq!(choice.id(), "arbitrary-model");
            assert!(choice.manual());
        }
    }
}

#[test]
fn model_picker_exposes_effort_without_waiting_for_discovery_and_escape_does_not_apply() {
    let mut model = model();
    model.slash_command("/model reviewer");
    key(&mut model, KeyCode::Char('e'));
    assert!(model.chat.effort_picker());
    key(&mut model, KeyCode::End);
    key(&mut model, KeyCode::Esc);
    assert_eq!(model.chat.models.reviewer().effort(), EffortLevel::Default);
    assert!(!model.chat.effort_picker());
    model.slash_command("/review");
    model.slash_command("/effort low");
    assert_eq!(model.chat.models.reviewer().effort(), EffortLevel::Low);
    assert_eq!(model.chat.models.writer().effort(), EffortLevel::Default);
}

#[test]
fn active_effort_update_waits_for_durable_ack_and_rejection_preserves_selection_and_draft() {
    for accepted in [false, true] {
        let mut model = model();
        let before = model_selection::active(&mut model);
        let effects = model.slash_command("/effort writer xhigh");
        let [Effect::Send(AppMessage::Request(request))] = effects.as_slice() else {
            panic!("update")
        };
        let AppRequestPayload::UpdateModels(update) = request.payload() else { panic!("models") };
        assert_eq!(update.models().writer().effort(), EffortLevel::XHigh);
        assert_eq!(model.chat.models.writer().effort(), EffortLevel::Default);
        model.paste_chat("retained draft");
        assert!(model.slash_command("/effort low").is_empty(), "no overlapping change");
        let payload = if accepted {
            AppResponsePayload::Interaction(
                ProductInteractionSnapshot::new(
                    before.snapshot().clone(),
                    before.mode(),
                    update.models().clone(),
                    1,
                    1,
                    Vec::new(),
                    None,
                )
                .expect("ack"),
            )
        } else {
            AppResponsePayload::Error(peritus_app_protocol::AppProtocolError::new(
                peritus_app_protocol::AppErrorCode::MissingRequiredFeature,
                None,
            ))
        };
        model.handle_message(AppMessage::Response(AppResponseEnvelope::new(
            request.context(),
            request.request_id(),
            request.correlation_id(),
            payload,
        )));
        assert_eq!(
            model.chat.models.writer().effort(),
            if accepted { EffortLevel::XHigh } else { EffortLevel::Default }
        );
        assert_eq!(model.chat.buffer, "retained draft");
        assert!(!model.chat_submission_pending());
        if !accepted {
            assert!(model.notice.as_ref().expect("error").text.contains("unsupported"));
        }
    }
}

#[test]
fn invalid_and_disconnected_effort_changes_never_claim_saved() {
    let mut model = model();
    for command in ["/effort banana", "/effort low extra", "/effort writer high extra"] {
        model.slash_command(command);
        assert!(model.notice.as_ref().expect("usage").text.starts_with("Usage:"));
        assert!(!model.chat.models.has_effort());
    }
    model_selection::active(&mut model);
    model.context = None;
    assert!(model.slash_command("/effort medium").is_empty());
    assert!(!model.chat.models.has_effort());
    assert!(model.notice.as_ref().expect("error").text.contains("not sent"));
}
