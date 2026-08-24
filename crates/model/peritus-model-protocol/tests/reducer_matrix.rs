//! C5 reducer acceptance matrix for item kinds, ordering, limits, and terminals.

use peritus_model_protocol::{
    EventEnvelope, EventId, FinishReason, ItemId, ItemKind, ModelEvent, ProtocolErrorKind,
    ProtocolLimits, ProviderName, ReducedItem, ReducerTransition, ResponseReducer, StreamFragment,
    TerminalOutcome, ToolCallId, ToolName,
};

fn reducer(limits: ProtocolLimits) -> ResponseReducer {
    ResponseReducer::new(ProviderName::new("matrix-provider".to_owned()).expect("provider"), limits)
}

fn envelope(sequence: u64, event: ModelEvent) -> EventEnvelope {
    let envelope = EventEnvelope::new(
        sequence,
        Some(sequence),
        Some(EventId::new(format!("matrix-event-{sequence}")).expect("event identity")),
        peritus_codec::sha256(&sequence.to_be_bytes()),
        event,
    )
    .expect("envelope");
    assert_eq!(envelope.protocol(), peritus_model_protocol::ProtocolVersion::V1);
    envelope
}

fn push(reducer: &mut ResponseReducer, sequence: u64, event: ModelEvent) -> ReducerTransition {
    reducer.push(envelope(sequence, event)).expect("accepted event")
}

fn fragment(bytes: &[u8], limits: ProtocolLimits) -> StreamFragment {
    StreamFragment::new(bytes.to_vec(), limits).expect("fragment")
}

#[test]
fn reasoning_structured_output_and_parallel_tools_reduce_in_index_order() {
    let limits = ProtocolLimits::PRODUCTION;
    let mut reducer = reducer(limits);
    let reasoning = ItemId::new("reasoning-item".to_owned()).expect("item");
    let structured = ItemId::new("structured-item".to_owned()).expect("item");
    let later_tool = ItemId::new("later-tool-item".to_owned()).expect("item");
    let earlier_tool = ItemId::new("earlier-tool-item".to_owned()).expect("item");
    let later_call = ToolCallId::new("later-tool-call".to_owned()).expect("call");
    let earlier_call = ToolCallId::new("earlier-tool-call".to_owned()).expect("call");

    push(&mut reducer, 1, ModelEvent::ResponseStarted { response_id: None, model: None });
    push(
        &mut reducer,
        2,
        ModelEvent::ItemStarted { item_id: reasoning.clone(), index: 0, kind: ItemKind::Reasoning },
    );
    push(
        &mut reducer,
        3,
        ModelEvent::ReasoningSummaryDelta {
            item_id: reasoning.clone(),
            fragment: fragment(b"checked", limits),
        },
    );
    push(
        &mut reducer,
        4,
        ModelEvent::ReasoningReplayDelta {
            item_id: reasoning.clone(),
            fragment: fragment(&[1, 2, 3], limits),
        },
    );
    push(&mut reducer, 5, ModelEvent::ItemCompleted(reasoning));
    push(
        &mut reducer,
        6,
        ModelEvent::ItemStarted {
            item_id: structured.clone(),
            index: 1,
            kind: ItemKind::StructuredOutput,
        },
    );
    push(
        &mut reducer,
        7,
        ModelEvent::TextDelta {
            item_id: structured.clone(),
            fragment: fragment(br#"{"ok":"#, limits),
        },
    );
    push(
        &mut reducer,
        8,
        ModelEvent::TextDelta { item_id: structured.clone(), fragment: fragment(b"true}", limits) },
    );
    push(&mut reducer, 9, ModelEvent::ItemCompleted(structured));

    for (sequence, item_id, index, call_id, name, arguments) in [
        (10, later_tool, 3, later_call, "second", br#"{"b":2}"#.as_slice()),
        (14, earlier_tool, 2, earlier_call, "first", br#"{"a":1}"#.as_slice()),
    ] {
        push(
            &mut reducer,
            sequence,
            ModelEvent::ItemStarted { item_id: item_id.clone(), index, kind: ItemKind::ToolCall },
        );
        push(
            &mut reducer,
            sequence + 1,
            ModelEvent::ToolCallStarted {
                item_id: item_id.clone(),
                call_id: call_id.clone(),
                name: ToolName::new(name.to_owned()).expect("tool name"),
            },
        );
        push(
            &mut reducer,
            sequence + 2,
            ModelEvent::ToolArgumentDelta { call_id, fragment: fragment(arguments, limits) },
        );
        push(&mut reducer, sequence + 3, ModelEvent::ItemCompleted(item_id));
    }
    push(&mut reducer, 18, ModelEvent::Finish(FinishReason::ToolCalls));
    let terminal = push(&mut reducer, 19, ModelEvent::ResponseCompleted);

    assert!(matches!(
        terminal,
        ReducerTransition::Terminal(TerminalOutcome::RequiresAction {
            reason: FinishReason::ToolCalls
        })
    ));
    assert_eq!(
        reducer.completed_items().iter().map(ReducedItem::index).collect::<Vec<_>>(),
        vec![0, 1, 2, 3]
    );
    assert!(matches!(reducer.completed_items()[0], ReducedItem::Reasoning { .. }));
    assert!(matches!(reducer.completed_items()[1], ReducedItem::Structured { .. }));
    assert!(matches!(reducer.completed_items()[2], ReducedItem::ToolCall { .. }));
    assert!(matches!(reducer.completed_items()[3], ReducedItem::ToolCall { .. }));
}

#[test]
fn refusal_and_cancellation_are_explicit_non_success_terminals() {
    let limits = ProtocolLimits::PRODUCTION;
    let mut refusal = reducer(limits);
    let item = ItemId::new("refusal-item".to_owned()).expect("item");
    push(&mut refusal, 1, ModelEvent::ResponseStarted { response_id: None, model: None });
    push(
        &mut refusal,
        2,
        ModelEvent::ItemStarted { item_id: item.clone(), index: 0, kind: ItemKind::Refusal },
    );
    push(
        &mut refusal,
        3,
        ModelEvent::RefusalDelta {
            item_id: item.clone(),
            fragment: fragment(b"cannot comply", limits),
        },
    );
    push(&mut refusal, 4, ModelEvent::ItemCompleted(item));
    push(&mut refusal, 5, ModelEvent::Finish(FinishReason::Refusal));
    assert!(matches!(
        push(&mut refusal, 6, ModelEvent::ResponseCompleted),
        ReducerTransition::Terminal(TerminalOutcome::Refused { .. })
    ));

    let mut cancelled = reducer(limits);
    push(&mut cancelled, 1, ModelEvent::ResponseStarted { response_id: None, model: None });
    assert_eq!(
        push(&mut cancelled, 2, ModelEvent::ResponseCancelled),
        ReducerTransition::Terminal(TerminalOutcome::Cancelled)
    );
    let original = cancelled.terminal().cloned();
    assert_eq!(
        cancelled
            .push(envelope(3, ModelEvent::Heartbeat))
            .expect_err("event after terminal")
            .kind(),
        ProtocolErrorKind::InvalidEvent
    );
    assert_eq!(cancelled.terminal(), original.as_ref());
}

#[test]
fn malformed_completed_fragments_irreversibly_fail_the_response() {
    let limits = ProtocolLimits::PRODUCTION;
    for (kind, bytes, expected_kind) in [
        (ItemKind::Message, vec![0xc3], ProtocolErrorKind::InvalidEvent),
        (
            ItemKind::StructuredOutput,
            br#"{"unterminated":"#.to_vec(),
            ProtocolErrorKind::InvalidSchema,
        ),
    ] {
        let mut reducer = reducer(limits);
        let item = ItemId::new(format!("malformed-{kind:?}")).expect("item");
        push(&mut reducer, 1, ModelEvent::ResponseStarted { response_id: None, model: None });
        push(&mut reducer, 2, ModelEvent::ItemStarted { item_id: item.clone(), index: 0, kind });
        push(
            &mut reducer,
            3,
            ModelEvent::TextDelta { item_id: item.clone(), fragment: fragment(&bytes, limits) },
        );
        assert_eq!(
            reducer
                .push(envelope(4, ModelEvent::ItemCompleted(item)))
                .expect_err("malformed item")
                .kind(),
            expected_kind
        );
        assert!(matches!(reducer.terminal(), Some(TerminalOutcome::Failed(_))));
        assert_eq!(
            reducer.finish_eof().expect("failure already terminal"),
            reducer.terminal().expect("terminal").clone()
        );
    }
}

#[test]
fn gaps_limits_open_items_and_missing_terminals_fail_closed() {
    let limits =
        ProtocolLimits::new([8, 8, 8, 8, 8, 8, 8, 16, 8, 8, 3, 8, 8]).expect("small limits");
    let item = ItemId::new("bounded-item".to_owned()).expect("item");
    let mut bounded = reducer(limits);
    push(&mut bounded, 1, ModelEvent::ResponseStarted { response_id: None, model: None });
    push(
        &mut bounded,
        2,
        ModelEvent::ItemStarted { item_id: item.clone(), index: 0, kind: ItemKind::Message },
    );
    push(
        &mut bounded,
        3,
        ModelEvent::TextDelta { item_id: item.clone(), fragment: fragment(b"ab", limits) },
    );
    assert_eq!(
        bounded
            .push(envelope(
                4,
                ModelEvent::TextDelta { item_id: item, fragment: fragment(b"cd", limits) },
            ))
            .expect_err("aggregate output limit")
            .kind(),
        ProtocolErrorKind::InvalidEvent
    );
    assert!(matches!(bounded.terminal(), Some(TerminalOutcome::Failed(_))));

    let mut gap = reducer(ProtocolLimits::PRODUCTION);
    push(&mut gap, 1, ModelEvent::ResponseStarted { response_id: None, model: None });
    assert_eq!(
        gap.push(envelope(3, ModelEvent::Heartbeat)).expect_err("sequence gap").kind(),
        ProtocolErrorKind::InvalidEvent
    );
    assert!(matches!(gap.terminal(), Some(TerminalOutcome::Failed(_))));

    let mut open = reducer(ProtocolLimits::PRODUCTION);
    let item = ItemId::new("open-item".to_owned()).expect("item");
    push(&mut open, 1, ModelEvent::ResponseStarted { response_id: None, model: None });
    push(
        &mut open,
        2,
        ModelEvent::ItemStarted { item_id: item, index: 0, kind: ItemKind::Message },
    );
    push(&mut open, 3, ModelEvent::Finish(FinishReason::Stop));
    assert_eq!(
        open.push(envelope(4, ModelEvent::ResponseCompleted))
            .expect_err("open item terminal")
            .kind(),
        ProtocolErrorKind::InvalidEvent
    );
    assert!(matches!(open.terminal(), Some(TerminalOutcome::Failed(_))));

    let mut interrupted = reducer(ProtocolLimits::PRODUCTION);
    push(&mut interrupted, 1, ModelEvent::ResponseStarted { response_id: None, model: None });
    assert_eq!(
        interrupted.finish_eof().expect_err("missing terminal").kind(),
        ProtocolErrorKind::IncompleteStream
    );
    assert!(matches!(interrupted.terminal(), Some(TerminalOutcome::Failed(_))));
}
