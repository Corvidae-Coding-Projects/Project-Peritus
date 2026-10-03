use super::*;
use peritus_app_protocol::{AppErrorCode, AppProtocolError, AppRequestEnvelope, ProductModelInfo};

fn query(model: &mut AppModel) -> AppRequestEnvelope {
    let effects = model.slash_command("/model");
    let [Effect::Send(AppMessage::Request(request))] = effects.as_slice() else {
        panic!("model discovery request")
    };
    request.clone()
}

fn catalog(model: &AppModel, id: &str) -> ProductModelCatalog {
    ProductModelCatalog::new(
        model.chat_providers().expect("providers").writer(),
        "configured".to_owned(),
        vec![ProductModelInfo::new(id.to_owned(), id.to_owned(), Some(true)).expect("model")],
        1,
        false,
        String::new(),
    )
    .expect("catalog")
}

fn reply(model: &mut AppModel, request: &AppRequestEnvelope, payload: AppResponsePayload) {
    model.handle_message(AppMessage::Response(AppResponseEnvelope::new(
        request.context(),
        request.request_id(),
        request.correlation_id(),
        payload,
    )));
}

#[test]
fn refresh_and_role_keys_share_one_pending_lookup_for_the_same_provider() {
    let mut model = model();
    let request = query(&mut model);
    for _ in 0..10 {
        assert!(key(&mut model, KeyCode::Char('r')).is_empty());
        assert!(key(&mut model, KeyCode::Tab).is_empty());
    }
    assert!(model.pending.contains_key(&request.request_id()));
    let discovered = catalog(&model, "current");
    reply(&mut model, &request, AppResponsePayload::Models(discovered));
    assert_eq!(model.chat.catalog.as_ref().expect("catalog").models()[0].id(), "current");
}

#[test]
fn dismissed_lookup_cannot_overwrite_a_reopened_picker_or_leave_a_timeout() {
    let mut model = model();
    let old = query(&mut model);
    key(&mut model, KeyCode::Esc);
    assert!(!model.pending.contains_key(&old.request_id()));
    assert!(!model.pending_started.contains_key(&old.request_id()));
    let current = query(&mut model);
    let fresh = catalog(&model, "fresh");
    reply(&mut model, &current, AppResponsePayload::Models(fresh));
    let stale = catalog(&model, "stale");
    reply(&mut model, &old, AppResponsePayload::Models(stale));
    assert_eq!(model.chat.catalog.as_ref().expect("catalog").models()[0].id(), "fresh");
    let notice = model.notice.as_ref().map(|notice| notice.text.clone());
    reply(
        &mut model,
        &old,
        AppResponsePayload::Error(AppProtocolError::new(AppErrorCode::Backpressure, None)),
    );
    assert!(model.chat.model_picker());
    assert_eq!(model.notice.as_ref().map(|notice| notice.text.clone()), notice);
}

#[test]
fn effort_navigation_and_model_submission_abandon_only_discovery() {
    for select in [false, true] {
        let mut model = model();
        active(&mut model);
        let old = query(&mut model);
        if select {
            let cached = catalog(&model, "selected");
            model.accept_model_catalog(cached);
            let effects = key(&mut model, KeyCode::Enter);
            let [Effect::Send(AppMessage::Request(update))] = effects.as_slice() else {
                panic!("model update")
            };
            assert!(model.pending.contains_key(&update.request_id()));
            assert!(model.chat_submission_pending());
        } else {
            key(&mut model, KeyCode::Char('e'));
            assert!(model.chat.effort_picker());
        }
        assert!(!model.pending.contains_key(&old.request_id()));
        assert!(!model.pending_started.contains_key(&old.request_id()));
    }
}
