//! Structured normalized-event generation with deterministic twin-reducer oracles.

use peritus_codec::sha256;
use peritus_model_protocol::{
    EventEnvelope, EventId, FinishReason, ItemId, ItemKind, ModelEvent, ProtocolLimits,
    ProviderName, ReducerTransition, ResponseReducer, StreamFragment, ToolCallId, ToolName,
    UsageCounters, UsageObservation, UsageScope,
};

const EVENT_LIMIT: usize = 32;

/// Checks ordering, identity, item, tool, usage, terminal, and EOF behavior.
pub fn check(bytes: &[u8]) {
    let limits = limits();
    let provider = ProviderName::new("discovery-provider".to_owned()).expect("provider name");
    let mut first = ResponseReducer::new(provider.clone(), limits);
    let mut second = ResponseReducer::new(provider, limits);
    let events = events(bytes, limits);

    for event in events {
        let completed_before = first.completed_items().to_vec();
        let usage_before = first.usage_high_water();
        let terminal_before = first.terminal().cloned();
        let left = first.push(event.clone());
        let right = second.push(event);
        assert_eq!(
            left.as_ref().map_err(peritus_model_protocol::ProtocolError::kind),
            right.as_ref().map_err(peritus_model_protocol::ProtocolError::kind),
            "identical reducers disagreed on a generated event",
        );
        if matches!(left, Ok(ReducerTransition::DuplicateIgnored)) {
            assert_eq!(first.completed_items(), completed_before);
            assert_eq!(first.usage_high_water(), usage_before);
            assert_eq!(first.terminal(), terminal_before.as_ref());
        }
        match &left {
            Ok(ReducerTransition::Terminal(outcome)) => {
                assert_eq!(first.terminal(), Some(outcome));
            }
            Err(_) => assert!(first.terminal().is_some(), "rejection did not fail closed"),
            Ok(ReducerTransition::Applied | ReducerTransition::DuplicateIgnored) => {}
        }
        assert_usage_monotonic(usage_before, first.usage_high_water());
        assert_completed_order(&first);
        assert_observations_equal(&first, &second);
    }

    if bytes.last().is_none_or(|byte| byte & 1 == 0) {
        let left = first.finish_eof();
        let right = second.finish_eof();
        assert_eq!(
            left.as_ref().map_err(peritus_model_protocol::ProtocolError::kind),
            right.as_ref().map_err(peritus_model_protocol::ProtocolError::kind),
            "identical reducers disagreed at EOF",
        );
        assert_observations_equal(&first, &second);
    }
}

fn events(bytes: &[u8], limits: ProtocolLimits) -> Vec<EventEnvelope> {
    let count = usize::from(bytes.first().copied().unwrap_or(0)) % EVENT_LIMIT + 1;
    let mut events: Vec<EventEnvelope> = Vec::with_capacity(count);
    for index in 0..count {
        let index_byte = u8::try_from(index).expect("event limit fits u8");
        let selector = bytes.get(index + 1).copied().unwrap_or(index_byte);
        if selector & 0x80 != 0
            && let Some(previous) = events.last()
        {
            events.push(previous.clone());
            continue;
        }
        events.push(envelope(index, selector, limits));
    }
    events
}

fn envelope(index: usize, selector: u8, limits: ProtocolLimits) -> EventEnvelope {
    let ordinary_sequence = u64::try_from(index + 1).expect("event limit fits u64");
    let sequence = match selector & 0x03 {
        1 => ordinary_sequence.saturating_add(1),
        2 => ordinary_sequence.saturating_sub(1).max(1),
        _ => ordinary_sequence,
    };
    let provider_sequence = match selector & 0x0c {
        0 => None,
        4 => Some(ordinary_sequence),
        8 => Some(ordinary_sequence.saturating_add(1)),
        _ => Some(ordinary_sequence.saturating_sub(1).max(1)),
    };
    let provider_event_id = match selector & 0x30 {
        0 => None,
        0x10 => Some(event_id(&format!("event-{index}"))),
        _ => Some(event_id(&format!("event-{}", selector % 4))),
    };
    EventEnvelope::new(
        sequence,
        provider_sequence,
        provider_event_id,
        sha256(&[selector, u8::try_from(index).expect("event limit fits u8")]),
        model_event(index, selector, limits),
    )
    .expect("generated sequence numbers are nonzero")
}

fn model_event(index: usize, selector: u8, limits: ProtocolLimits) -> ModelEvent {
    let item = item_id(selector);
    let call = call_id(selector);
    match selector % 12 {
        0 => ModelEvent::ResponseStarted { response_id: None, model: None },
        1 => ModelEvent::Heartbeat,
        2 => ModelEvent::ItemStarted {
            item_id: item,
            index: u32::from(selector % 4),
            kind: ItemKind::Message,
        },
        3 => ModelEvent::TextDelta { item_id: item, fragment: fragment(selector, limits) },
        4 => ModelEvent::ItemCompleted(item),
        5 => ModelEvent::ItemStarted {
            item_id: item,
            index: u32::from(selector % 4),
            kind: ItemKind::ToolCall,
        },
        6 => ModelEvent::ToolCallStarted {
            item_id: item,
            call_id: call,
            name: ToolName::new(format!("tool-{}", selector % 3)).expect("tool name"),
        },
        7 => ModelEvent::ToolArgumentDelta {
            call_id: call,
            fragment: StreamFragment::new(b"{}".to_vec(), limits).expect("bounded JSON fragment"),
        },
        8 => ModelEvent::Usage(usage(index, selector)),
        9 => ModelEvent::Finish(FinishReason::Stop),
        10 => ModelEvent::ResponseCompleted,
        _ => ModelEvent::ResponseCancelled,
    }
}

fn usage(index: usize, selector: u8) -> UsageObservation {
    let value = u64::try_from(index).expect("event limit fits u64") + u64::from(selector % 3);
    let scope = match selector & 0x03 {
        0 => UsageScope::Step,
        1 => UsageScope::Cumulative,
        _ => UsageScope::Final,
    };
    UsageObservation::new(
        scope,
        UsageCounters::new(Some(value), None, None, Some(value), None, None, None, None),
        None,
    )
}

fn fragment(selector: u8, limits: ProtocolLimits) -> StreamFragment {
    let bytes = if selector & 0x40 == 0 { vec![selector] } else { vec![0xf0, 0x9f] };
    StreamFragment::new(bytes, limits).expect("bounded nonempty fragment")
}

fn item_id(selector: u8) -> ItemId {
    ItemId::new(format!("item-{}", selector % 3)).expect("item identity")
}

fn call_id(selector: u8) -> ToolCallId {
    ToolCallId::new(format!("call-{}", selector % 3)).expect("call identity")
}

fn event_id(value: &str) -> EventId {
    EventId::new(value.to_owned()).expect("event identity")
}

fn limits() -> ProtocolLimits {
    ProtocolLimits::new([8, 16, 1_024, 1_024, 2_048, 8, 1_024, 64, 8, 256, 2_048, 1_024, 1_024])
        .expect("bounded protocol limits")
}

fn assert_observations_equal(first: &ResponseReducer, second: &ResponseReducer) {
    assert_eq!(first.completed_items(), second.completed_items());
    assert_eq!(first.usage_high_water(), second.usage_high_water());
    assert_eq!(first.terminal(), second.terminal());
    assert_eq!(first.response_id(), second.response_id());
    assert_eq!(first.rate_limits(), second.rate_limits());
    assert_eq!(first.cache_observations(), second.cache_observations());
    assert_eq!(first.provider_events(), second.provider_events());
}

fn assert_completed_order(reducer: &ResponseReducer) {
    assert!(
        reducer.completed_items().windows(2).all(|pair| pair[0].index() < pair[1].index()),
        "completed provider items are not in unique index order",
    );
}

fn assert_usage_monotonic(before: UsageCounters, after: UsageCounters) {
    for (old, new) in [
        (before.input_tokens(), after.input_tokens()),
        (before.cached_input_tokens(), after.cached_input_tokens()),
        (before.cache_creation_input_tokens(), after.cache_creation_input_tokens()),
        (before.output_tokens(), after.output_tokens()),
        (before.reasoning_output_tokens(), after.reasoning_output_tokens()),
        (before.tool_tokens(), after.tool_tokens()),
        (before.total_tokens(), after.total_tokens()),
        (before.provider_cost_microunits(), after.provider_cost_microunits()),
    ] {
        if let Some(old) = old {
            assert!(new.is_some_and(|new| new >= old), "usage high-water regressed or disappeared");
        }
    }
}
