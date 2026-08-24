//! Direct normalized stream, failure, and accounting observations.

use peritus_conformance::{
    ProviderConformanceError, ProviderConformanceObservation, ProviderEventKind,
    ProviderEventObservation, ProviderFailureKind, ProviderFailureObservation,
    ProviderStreamObservation, ProviderTerminal, ProviderUsageObservation, ProviderUsageSnapshot,
};
use peritus_model_protocol::{EventEnvelope, FailureCategory, ModelEvent};

use super::Probe;

pub(super) fn ordered(probe: &Probe) -> ProviderConformanceObservation {
    let events = observed_events(&probe.events);
    let received = u64::try_from(events.len()).expect("bounded") + probe.duplicate_events;
    ProviderConformanceObservation::Stream(ProviderStreamObservation::new(
        events,
        received,
        probe.duplicate_events,
        terminal_count(&probe.events),
        None,
        None,
        None,
    ))
}

pub(super) fn fragmented(
    probe: &Probe,
    expected: [u8; 32],
) -> Result<ProviderConformanceObservation, ProviderConformanceError> {
    let mut bytes = Vec::new();
    for event in &probe.events {
        if let ModelEvent::ToolArgumentDelta { fragment, .. } = event.event() {
            bytes.extend_from_slice(fragment.expose());
        }
    }
    let parsed: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| ProviderConformanceError::Infrastructure)?;
    if parsed.get("city").and_then(serde_json::Value::as_str) != Some("Paris")
        || parsed.as_object().is_none_or(|object| object.len() != 1)
        || peritus_codec::sha256(&bytes).into_bytes() == [0; 32]
    {
        return Err(ProviderConformanceError::Infrastructure);
    }
    let events = observed_events(&probe.events);
    let final_fragment = events
        .iter()
        .rev()
        .find(|event| event.kind() == ProviderEventKind::ToolArgumentDelta)
        .map(|event| event.sequence());
    let closed = events
        .iter()
        .rev()
        .find(|event| event.kind() == ProviderEventKind::ItemCompleted)
        .map(|event| event.sequence());
    let received =
        u64::try_from(events.len()).map_err(|_| ProviderConformanceError::Infrastructure)?;
    Ok(ProviderConformanceObservation::Stream(ProviderStreamObservation::new(
        events,
        received,
        0,
        terminal_count(&probe.events),
        Some(expected),
        final_fragment,
        closed,
    )))
}

pub(super) fn failure(
    probe: &Probe,
    kind: ProviderFailureKind,
    category: FailureCategory,
) -> Result<ProviderConformanceObservation, ProviderConformanceError> {
    let Some(ModelEvent::ResponseFailed(actual)) = probe.events.last().map(EventEnvelope::event)
    else {
        return Err(ProviderConformanceError::Infrastructure);
    };
    if actual.category() != category {
        return Err(ProviderConformanceError::Infrastructure);
    }
    let partial = probe.events.iter().filter(|event| !is_terminal(event.event())).count();
    Ok(ProviderConformanceObservation::Failure(ProviderFailureObservation::new(
        kind,
        ProviderTerminal::Failed,
        probe.transport_requests,
        u64::try_from(partial).map_err(|_| ProviderConformanceError::Infrastructure)?,
    )))
}

pub(super) fn usage(events: &[EventEnvelope]) -> ProviderConformanceObservation {
    let snapshots = events
        .iter()
        .filter_map(|event| match event.event() {
            ModelEvent::Usage(value) => {
                let counters = value.counters();
                Some(ProviderUsageSnapshot::new(
                    counters.input_tokens(),
                    counters.cached_input_tokens(),
                    counters.output_tokens(),
                    counters.total_tokens(),
                ))
            }
            _ => None,
        })
        .collect();
    ProviderConformanceObservation::Usage(ProviderUsageObservation::new(snapshots))
}

fn observed_events(events: &[EventEnvelope]) -> Vec<ProviderEventObservation> {
    events
        .iter()
        .filter_map(|event| {
            let (kind, bytes) = match event.event() {
                ModelEvent::ResponseStarted { .. } => (ProviderEventKind::ResponseStarted, 0),
                ModelEvent::ItemStarted { .. } => (ProviderEventKind::ItemStarted, 0),
                ModelEvent::TextDelta { fragment, .. } => {
                    (ProviderEventKind::TextDelta, fragment.len())
                }
                ModelEvent::ToolCallStarted { .. } => (ProviderEventKind::ToolCallStarted, 0),
                ModelEvent::ToolArgumentDelta { fragment, .. } => {
                    (ProviderEventKind::ToolArgumentDelta, fragment.len())
                }
                ModelEvent::ItemCompleted(_) => (ProviderEventKind::ItemCompleted, 0),
                ModelEvent::Usage(_) => (ProviderEventKind::Usage, 0),
                ModelEvent::Finish(_) => (ProviderEventKind::Finish, 0),
                ModelEvent::ResponseCompleted => (ProviderEventKind::ResponseCompleted, 0),
                ModelEvent::ResponseFailed(_) => (ProviderEventKind::ResponseFailed, 0),
                ModelEvent::ResponseCancelled => (ProviderEventKind::ResponseCancelled, 0),
                _ => return None,
            };
            let digest = event.provider_digest().into_bytes();
            Some(ProviderEventObservation::new(
                event.sequence(),
                event.provider_sequence(),
                digest,
                digest,
                kind,
                u64::try_from(bytes).expect("bounded"),
            ))
        })
        .collect()
}

pub(super) fn terminal_count(events: &[EventEnvelope]) -> u64 {
    u64::try_from(events.iter().filter(|event| is_terminal(event.event())).count())
        .expect("bounded")
}

pub(super) const fn is_terminal(event: &ModelEvent) -> bool {
    matches!(
        event,
        ModelEvent::ResponseCompleted
            | ModelEvent::ResponseFailed(_)
            | ModelEvent::ResponseCancelled
    )
}
