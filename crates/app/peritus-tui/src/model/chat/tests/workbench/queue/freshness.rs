use super::*;

#[test]
fn abandoned_queue_actions_cannot_mutate_on_late_metadata() {
    for reason in ["draft", "escape", "selection", "timeout"] {
        let mut model = opened();
        inspect(&mut model);
        key(&mut model, KeyCode::Esc);
        model.chat.buffer = "/queue hold 1".into();
        let refresh = request(&key(&mut model, KeyCode::Enter));
        match reason {
            "draft" => model.chat.buffer = "Changed my mind λ".into(),
            "escape" => {
                key(&mut model, KeyCode::Esc);
            }
            "selection" => model.select_workbench_conversation(None),
            "timeout" => {
                model.pending_started.retain(|id, _| *id == refresh.request_id());
                model.tick_count = 119;
                let effects = model.update(Action::Tick(std::time::Instant::now()));
                assert!(!effects.iter().any(|effect| matches!(effect, Effect::Reconnect)));
                assert!(!model.pending.contains_key(&refresh.request_id()));
            }
            _ => unreachable!(),
        }
        let draft = model.chat.buffer.clone();
        assert!(fresh_metadata(&mut model, &refresh, 8).is_empty(), "{reason}");
        assert_eq!(model.chat.buffer, draft, "{reason}");
        assert!(model.chat.workbench.unresolved.is_none(), "{reason}");
        assert!(matches!(model.connection, crate::model::ConnectionStatus::Online { .. }));
    }
}

#[test]
fn a_replaced_queue_page_cannot_retarget_an_already_selected_action() {
    let mut model = opened();
    inspect(&mut model);
    key(&mut model, KeyCode::Esc);
    model.chat.buffer = "/queue hold 1".into();
    let refresh = request(&key(&mut model, KeyCode::Enter));
    let page = model.chat.workbench.queue.as_ref().unwrap();
    let replacement = WorkbenchInputRow::new(
        WorkbenchInputSelection::new(WorkbenchInputId::new([73; 16]).unwrap(), 1).unwrap(),
        WorkbenchInputText::new("Another input now occupies row one".into()).unwrap(),
        WorkbenchInputState::Queued,
        WorkbenchInputOrder::new(Vec::new()).unwrap(),
    )
    .unwrap();
    model.chat.workbench.queue =
        Some(WorkbenchQueuePage::new(page.query(), 1, vec![replacement]).unwrap());
    let sent = request(&fresh_metadata(&mut model, &refresh, 8));
    let AppRequestPayload::WorkbenchCommand(command) = sent.payload() else {
        panic!("queue command")
    };
    assert_eq!(command.expected_revision(), 8);
    assert!(matches!(command.intent(),
        WorkbenchIntent::Queue(WorkbenchQueueIntent::Hold { selected, held: true })
        if *selected == row(WorkbenchInputState::Queued).selected()));
}

#[test]
fn explicit_queue_inspection_clears_the_previous_rejection_message() {
    let mut model = opened();
    model.chat.workbench.message = "Rejected: stale revision".into();
    inspect(&mut model);
    assert!(model.chat.workbench.message.is_empty());
}
