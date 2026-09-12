//! Structured normalized-event generation with invariants and a twin-reducer determinism check.

use peritus_codec::sha256;
use peritus_model_protocol::{
    EventEnvelope, EventId, FinishReason, ItemId, ItemKind, ModelEvent, ProtocolLimits,
    ProviderName, ReducerTransition, ResponseReducer, StreamFragment, ToolCallId, ToolName,
    UsageCounters, UsageObservation, UsageScope,
};

const EVENT_LIMIT: usize = 32;
const EVENT_BYTES: usize = 6;

/// Checks ordering, identity, item, tool, usage, terminal, and EOF behavior.
pub fn check(bytes: &[u8]) {
    let _ = reduce(bytes);
}

fn reduce(bytes: &[u8]) -> ResponseReducer {
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
        // Owner contract: reducer_matrix::refusal_and_cancellation_are_explicit_non_success_terminals.
        // A second reducer can repeat the same bug, so check terminal stability directly.
        if terminal_before.is_some() {
            assert_eq!(
                left.as_ref().expect_err("event after terminal").kind(),
                peritus_model_protocol::ProtocolErrorKind::InvalidEvent,
            );
            assert_eq!(first.terminal(), terminal_before.as_ref());
        }
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
    first
}

fn events(bytes: &[u8], limits: ProtocolLimits) -> Vec<EventEnvelope> {
    let count = usize::from(bytes.first().copied().unwrap_or(0) & 0x1f) % EVENT_LIMIT + 1;
    let mut events: Vec<EventEnvelope> = Vec::with_capacity(count);
    for index in 0..count {
        let encoded = EncodedEvent::read(bytes, index);
        if encoded.control & 0x80 != 0
            && let Some(previous) = events.last()
        {
            events.push(previous.clone());
            continue;
        }
        events.push(envelope(index, encoded, limits));
    }
    events
}

#[derive(Clone, Copy)]
struct EncodedEvent {
    control: u8,
    kind: u8,
    identities: u8,
    output_index: u8,
    ordering: u8,
    payload: u8,
}

impl EncodedEvent {
    fn read(bytes: &[u8], index: usize) -> Self {
        let base = 1 + index * EVENT_BYTES;
        let value = |offset| {
            bytes
                .get(base + offset)
                .copied()
                .unwrap_or_else(|| u8::try_from(index + offset).expect("event limit fits u8"))
        };
        Self {
            control: value(0),
            kind: value(1),
            identities: value(2),
            output_index: value(3),
            ordering: value(4),
            payload: value(5),
        }
    }
}

fn envelope(index: usize, encoded: EncodedEvent, limits: ProtocolLimits) -> EventEnvelope {
    let ordinary_sequence = u64::try_from(index + 1).expect("event limit fits u64");
    let sequence = match encoded.ordering & 0x03 {
        1 => ordinary_sequence.saturating_add(1),
        2 => ordinary_sequence.saturating_sub(1).max(1),
        _ => ordinary_sequence,
    };
    let provider_sequence = match (encoded.ordering >> 2) & 0x03 {
        0 => None,
        1 => Some(ordinary_sequence),
        2 => Some(ordinary_sequence.saturating_add(1)),
        _ => Some(ordinary_sequence.saturating_sub(1).max(1)),
    };
    let provider_event_id = match encoded.control & 0x03 {
        0 => None,
        1 => Some(event_id(&format!("event-{index}"))),
        2 => Some(event_id(&format!("event-{}", encoded.control >> 2 & 0x03))),
        _ => Some(event_id(&format!("event-{}", index.saturating_sub(1)))),
    };
    let index_byte = u8::try_from(index).expect("event limit fits u8");
    EventEnvelope::new(
        sequence,
        provider_sequence,
        provider_event_id,
        sha256(&[
            encoded.control,
            encoded.kind,
            encoded.identities,
            encoded.output_index,
            encoded.ordering,
            encoded.payload,
            index_byte,
        ]),
        model_event(index, encoded, limits),
    )
    .expect("generated sequence numbers are nonzero")
}

fn model_event(index: usize, encoded: EncodedEvent, limits: ProtocolLimits) -> ModelEvent {
    let item = item_id(encoded.identities & 0x03);
    let call = call_id((encoded.identities >> 2) & 0x03);
    match encoded.kind % 12 {
        0 => ModelEvent::ResponseStarted { response_id: None, model: None },
        1 => ModelEvent::Heartbeat,
        2 => ModelEvent::ItemStarted {
            item_id: item,
            index: u32::from(encoded.output_index % 4),
            kind: ItemKind::Message,
        },
        3 => ModelEvent::TextDelta { item_id: item, fragment: fragment(encoded.payload, limits) },
        4 => ModelEvent::ItemCompleted(item),
        5 => ModelEvent::ItemStarted {
            item_id: item,
            index: u32::from(encoded.output_index % 4),
            kind: ItemKind::ToolCall,
        },
        6 => ModelEvent::ToolCallStarted {
            item_id: item,
            call_id: call,
            name: ToolName::new(format!("tool-{}", encoded.payload % 3)).expect("tool name"),
        },
        7 => ModelEvent::ToolArgumentDelta {
            call_id: call,
            fragment: tool_fragment(encoded.payload, limits),
        },
        8 => ModelEvent::Usage(usage(index, encoded.payload)),
        9 => ModelEvent::Finish(match encoded.payload % 4 {
            0 => FinishReason::Stop,
            1 => FinishReason::ToolCalls,
            2 => FinishReason::Cancelled,
            _ => FinishReason::Length,
        }),
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
    let bytes = if selector & 0x80 == 0 { vec![b'a' + selector % 26] } else { vec![0xf0, 0x9f] };
    StreamFragment::new(bytes, limits).expect("bounded nonempty fragment")
}

fn tool_fragment(selector: u8, limits: ProtocolLimits) -> StreamFragment {
    let bytes = match selector % 4 {
        0 => b"{}".to_vec(),
        1 => b"{\"value\":".to_vec(),
        2 => b"1}".to_vec(),
        _ => b"[]".to_vec(),
    };
    StreamFragment::new(bytes, limits).expect("bounded JSON fragment")
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

#[cfg(test)]
mod tests;
