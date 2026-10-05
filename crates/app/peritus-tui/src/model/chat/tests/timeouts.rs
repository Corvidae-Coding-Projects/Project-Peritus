use super::*;
use peritus_app_protocol::{
    AppRequestEnvelope, ConversationId, ConversationTitle, ProtocolFeatureName,
    WellKnownProtocolFeature, WorkbenchQuery, WorkbenchSnapshot,
};

fn request(effects: &[Effect]) -> AppRequestEnvelope {
    let [Effect::Send(AppMessage::Request(request))] = effects else { panic!("one request") };
    request.clone()
}

fn wait_without_expiry(model: &mut AppModel, request: &AppRequestEnvelope) -> Vec<Effect> {
    model.pending_started.retain(|id, _| *id == request.request_id());
    model.tick_count = 10_000;
    model.update(Action::Tick(std::time::Instant::now()))
}

#[test]
fn slow_read_only_panels_remain_retryable_without_reconnecting_the_conversation() {
    for command in [
        "/doctor",
        "/model",
        "/sessions",
        "/brief",
        "/queue",
        "/context",
        "/memory",
        "/permissions",
        "/goal",
        "/init",
    ] {
        let mut model = model();
        model.features = [
            WellKnownProtocolFeature::ProductDiagnostics,
            WellKnownProtocolFeature::WorkbenchControl,
            WellKnownProtocolFeature::ConversationLibrary,
            WellKnownProtocolFeature::WorkbenchBrief,
            WellKnownProtocolFeature::WorkbenchInputs,
            WellKnownProtocolFeature::WorkbenchContext,
            WellKnownProtocolFeature::WorkbenchMemory,
            WellKnownProtocolFeature::WorkbenchPermissions,
            WellKnownProtocolFeature::WorkbenchGoals,
            WellKnownProtocolFeature::WorkbenchInit,
        ]
        .into_iter()
        .map(|feature| ProtocolFeatureName::well_known(feature).unwrap())
        .collect();
        let scope = WorkbenchQuery::new(
            ConversationId::new([79; 16]).unwrap(),
            model.product.as_ref().unwrap().launch.workspace_id(),
        );
        model.chat.workbench.selected = Some(scope);
        model.chat.workbench.snapshot = Some(
            WorkbenchSnapshot::new(
                scope,
                1,
                ConversationTitle::new("Slow session".into()).unwrap(),
                false,
                false,
            )
            .unwrap(),
        );
        let old = request(&model.slash_command(command));
        model.chat.buffer = "Keep the next draft 界".into();
        let context = model.context;
        let effects = wait_without_expiry(&mut model, &old);
        assert!(!effects.iter().any(|effect| matches!(effect, Effect::Reconnect)), "{command}");
        assert_eq!(model.context, context);
        assert!(matches!(model.connection, crate::model::ConnectionStatus::Online { .. }));
        assert!(model.pending.contains_key(&old.request_id()));
        assert_eq!(model.chat.buffer, "Keep the next draft 界");
    }
}

#[test]
fn a_slow_diagnostic_does_not_interrupt_an_unexpired_model_update() {
    let mut model = model();
    model_selection::active(&mut model);
    model.features = vec![
        ProtocolFeatureName::well_known(WellKnownProtocolFeature::ProductDiagnostics).unwrap(),
    ];
    let diagnostic = request(&model.slash_command("/doctor"));
    model.tick_count = 60;
    let update = request(&model.slash_command("/model manual replacement"));
    model
        .pending_started
        .retain(|id, _| *id == diagnostic.request_id() || *id == update.request_id());
    model.tick_count = 10_000;
    let effects = model.update(Action::Tick(std::time::Instant::now()));
    assert!(!effects.iter().any(|effect| matches!(effect, Effect::Reconnect)));
    assert!(model.pending.contains_key(&update.request_id()));
    assert!(model.chat_submission_pending());
    assert!(model.chat.doctor.as_ref().unwrap().error.is_none());
    model.tick_count = 179;
    let effects = model.update(Action::Tick(std::time::Instant::now()));
    assert!(!effects.iter().any(|effect| matches!(effect, Effect::Reconnect)));
    assert!(model.pending.contains_key(&diagnostic.request_id()));
    assert!(model.pending.contains_key(&update.request_id()));
}
