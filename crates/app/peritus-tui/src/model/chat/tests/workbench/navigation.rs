use super::*;

#[test]
fn dismissing_a_slow_inspector_allows_a_new_task_without_waiting_for_its_reply() {
    for command in
        ["/sessions", "/brief", "/queue", "/context", "/memory", "/permissions", "/goal", "/init"]
    {
        let mut model = enabled_model();
        enable_durable_chat(&mut model);
        model.features.extend(
            [
                WellKnownProtocolFeature::ConversationLibrary,
                WellKnownProtocolFeature::WorkbenchBrief,
                WellKnownProtocolFeature::WorkbenchContext,
                WellKnownProtocolFeature::WorkbenchMemory,
                WellKnownProtocolFeature::WorkbenchPermissions,
                WellKnownProtocolFeature::WorkbenchGoals,
                WellKnownProtocolFeature::WorkbenchInit,
            ]
            .into_iter()
            .map(|feature| ProtocolFeatureName::well_known(feature).unwrap()),
        );
        let scope = peritus_app_protocol::WorkbenchQuery::new(
            peritus_app_protocol::ConversationId::new([79; 16]).unwrap(),
            model.product.as_ref().unwrap().launch.workspace_id(),
        );
        model.chat.workbench.selected = Some(scope);
        let snapshot = WorkbenchSnapshot::new(
            scope,
            1,
            ConversationTitle::new("Slow session".into()).unwrap(),
            false,
            false,
        )
        .unwrap();
        model.chat.workbench.snapshot = Some(snapshot.clone());
        model.chat.buffer = command.into();
        let old = request(&key(&mut model, KeyCode::Enter));
        key(&mut model, KeyCode::Esc);
        assert!(
            !model.pending.contains_key(&old.request_id()),
            "{command} still blocks input after Escape"
        );
        assert!(!model.pending_started.contains_key(&old.request_id()));
        assert_eq!(model.chat.buffer, command, "dismissal lost the draft");
        model.chat.buffer = "/new".into();
        assert!(key(&mut model, KeyCode::Enter).is_empty());
        model.chat.buffer = "Actually build a different project 界".into();
        let next = request(&key(&mut model, KeyCode::Enter));
        assert!(matches!(next.payload(), AppRequestPayload::WorkbenchCommand(command)
            if matches!(command.intent(), peritus_app_protocol::WorkbenchIntent::CreateConversation(_))));
        let selected = model.chat.workbench.selected;
        assert_ne!(selected, Some(scope));
        assert!(respond(&mut model, &old, AppResponsePayload::Workbench(snapshot)).is_empty());
        assert_eq!(model.chat.workbench.selected, selected);
        assert_eq!(model.chat.buffer, "Actually build a different project 界");
        assert!(
            model.pending.contains_key(&next.request_id()),
            "old response disturbed the new operation"
        );
    }
}

#[test]
fn escaping_a_submitted_change_keeps_its_receipt_tracking_and_blocks_retargeting() {
    let mut model = enabled_model();
    let (sent, command) = create(&mut model);
    let selected = model.chat.workbench.selected;
    key(&mut model, KeyCode::Esc);
    assert!(!model.chat.workbench.open);
    assert!(model.pending.contains_key(&sent.request_id()));
    assert!(model.pending_started.contains_key(&sent.request_id()));
    model.chat.buffer = "/new".into();
    assert!(key(&mut model, KeyCode::Enter).is_empty());
    assert_eq!(model.chat.workbench.selected, selected);
    assert!(model.chat.workbench.unresolved.is_some());
    model.chat.buffer = "Keep this next draft".into();
    respond(&mut model, &sent, receipt(&command));
    assert_eq!(model.chat.buffer, "Keep this next draft");
    assert!(model.chat.workbench.unresolved.is_none());
}
