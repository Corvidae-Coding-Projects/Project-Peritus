#![allow(dead_code, reason = "shared focused-test fixtures vary by test binary")]

use peritus_trace::{CausalBinding, Observation, ObservationKind, ObservedTime, SpanId, TraceId};
use peritus_types::{EventId, SessionId};

pub fn event(value: u8) -> EventId {
    EventId::new([value; 16]).expect("nonzero event fixture")
}

pub fn trace(value: u8) -> TraceId {
    TraceId::new([value; 16]).expect("nonzero trace fixture")
}

pub fn span(value: u8) -> SpanId {
    SpanId::new([value; 8]).expect("nonzero span fixture")
}

pub fn binding(value: u8) -> CausalBinding {
    CausalBinding::session(SessionId::new([value; 16]).expect("nonzero session fixture"))
}

#[allow(clippy::too_many_arguments, reason = "trace fixtures keep causal fields visible")]
pub fn observation(
    event_value: u8,
    trace_id: TraceId,
    span_id: SpanId,
    span_sequence: u64,
    parent_span_id: Option<SpanId>,
    causal_events: Vec<EventId>,
    binding: CausalBinding,
    time: u64,
    kind: ObservationKind,
) -> Observation {
    Observation::new(
        event(event_value),
        trace_id,
        span_id,
        span_sequence,
        parent_span_id,
        causal_events,
        binding,
        ObservedTime::new(time, time).expect("nonzero observation time"),
        kind,
        Vec::new(),
        Vec::new(),
    )
    .expect("valid trace fixture")
}
