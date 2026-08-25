//! Exact span lifecycle and parent-transition fold rules.

use crate::{Observation, ObservationKind, SpanKind, SpanOutcome, TraceError, TraceErrorKind};

use super::state::{SpanSnapshot, TraceSnapshot};

pub(super) fn apply_span(
    snapshot: &mut TraceSnapshot,
    observation: &Observation,
) -> Result<(), TraceError> {
    match observation.kind() {
        ObservationKind::SpanStarted(kind) => start_span(snapshot, observation, kind),
        ObservationKind::Diagnostic(_) => advance_span(snapshot, observation, None),
        ObservationKind::SpanEnded(outcome) => advance_span(snapshot, observation, Some(outcome)),
    }
}

fn start_span(
    snapshot: &mut TraceSnapshot,
    observation: &Observation,
    kind: SpanKind,
) -> Result<(), TraceError> {
    if snapshot.spans.contains_key(&observation.span_id()) {
        return Err(transition("span identity was already opened"));
    }
    if let Some(parent_id) = observation.parent_span_id() {
        let parent = snapshot
            .spans
            .get(&parent_id)
            .ok_or_else(|| causal("structural parent span is absent"))?;
        if !parent.is_open() {
            return Err(transition("a child span cannot open under a closed parent"));
        }
        if !observation.binding().refines(parent.binding) {
            return Err(causal("child span binding does not refine its parent"));
        }
        if observation.causal_events().binary_search(&parent.latest_event).is_err() {
            return Err(causal("child start does not name the parent span's latest event"));
        }
    }
    snapshot.spans.insert(
        observation.span_id(),
        SpanSnapshot {
            span_id: observation.span_id(),
            parent_span_id: observation.parent_span_id(),
            kind,
            binding: observation.binding(),
            sequence: 1,
            start: observation.time(),
            latest: observation.time(),
            latest_event: observation.event_id(),
            outcome: None,
        },
    );
    Ok(())
}

fn advance_span(
    snapshot: &mut TraceSnapshot,
    observation: &Observation,
    outcome: Option<SpanOutcome>,
) -> Result<(), TraceError> {
    let span = snapshot
        .spans
        .get_mut(&observation.span_id())
        .ok_or_else(|| transition("span event arrived before span start"))?;
    if !span.is_open() {
        return Err(transition("closed span received another observation"));
    }
    let expected =
        span.sequence.checked_add(1).ok_or_else(|| sequence("span sequence overflow"))?;
    if observation.span_sequence() != expected
        || observation.parent_span_id() != span.parent_span_id
        || observation.binding() != span.binding
    {
        return Err(sequence("span sequence, parent, or binding changed"));
    }
    if observation.causal_events().binary_search(&span.latest_event).is_err() {
        return Err(causal("span observation omits its exact predecessor"));
    }
    if observation.time().monotonic_tick() < span.latest.monotonic_tick()
        || observation.time().unix_nanos() < span.latest.unix_nanos()
    {
        return Err(TraceError::static_error(
            TraceErrorKind::TimeRegression,
            "apply trace observation",
            "observed time regressed within a span",
        ));
    }
    span.sequence = expected;
    span.latest = observation.time();
    span.latest_event = observation.event_id();
    span.outcome = outcome;
    Ok(())
}

const fn causal(detail: &'static str) -> TraceError {
    TraceError::static_error(TraceErrorKind::CausalIntegrity, "apply trace observation", detail)
}

const fn sequence(detail: &'static str) -> TraceError {
    TraceError::static_error(TraceErrorKind::Sequence, "apply trace observation", detail)
}

const fn transition(detail: &'static str) -> TraceError {
    TraceError::static_error(TraceErrorKind::InvalidTransition, "apply trace observation", detail)
}
