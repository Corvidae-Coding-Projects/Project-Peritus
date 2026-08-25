#![allow(dead_code, reason = "shared focused-test fixtures vary by test binary")]

use peritus_telemetry::{
    ExportRecord, MetricName, MetricPoint, TelemetryProjection, project_telemetry,
};
use peritus_trace::{
    CausalBinding, DiagnosticCode, Observation, ObservationKind, ObservedTime, SpanId, SpanKind,
    SpanOutcome, TraceId, TraceProjectionState,
};
use peritus_types::{EventId, SessionId};

pub fn metric_record(value: u64) -> ExportRecord {
    ExportRecord::Metric(MetricPoint::new(
        MetricName::Retries,
        value,
        ObservedTime::new(value + 1, value + 1).expect("metric time"),
        TraceId::new([11; 16]).expect("metric trace"),
    ))
}

pub fn projection(code: DiagnosticCode) -> TelemetryProjection {
    let trace_id = TraceId::new([21; 16]).expect("trace");
    let span_id = SpanId::new([22; 8]).expect("span");
    let binding = CausalBinding::session(SessionId::new([23; 16]).expect("session"));
    let start = observation(
        24,
        trace_id,
        span_id,
        1,
        Vec::new(),
        binding,
        1,
        ObservationKind::SpanStarted(SpanKind::Recovery),
    );
    let diagnostic = observation(
        25,
        trace_id,
        span_id,
        2,
        vec![event(24)],
        binding,
        2,
        ObservationKind::Diagnostic(code),
    );
    let end = observation(
        26,
        trace_id,
        span_id,
        3,
        vec![event(25)],
        binding,
        3,
        ObservationKind::SpanEnded(SpanOutcome::Ok),
    );
    let mut trace = TraceProjectionState::default();
    trace.apply(start, 1).expect("start");
    trace.apply(diagnostic, 2).expect("diagnostic");
    trace.apply(end, 3).expect("end");
    project_telemetry(&trace).expect("telemetry projection")
}

fn event(value: u8) -> EventId {
    EventId::new([value; 16]).expect("event")
}

#[allow(clippy::too_many_arguments, reason = "trace fixture exposes exact causal fields")]
fn observation(
    event_value: u8,
    trace_id: TraceId,
    span_id: SpanId,
    span_sequence: u64,
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
        None,
        causal_events,
        binding,
        ObservedTime::new(time, time).expect("time"),
        kind,
        Vec::new(),
        Vec::new(),
    )
    .expect("observation")
}
