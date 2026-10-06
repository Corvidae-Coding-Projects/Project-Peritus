//! Direct observations from production `Codex` runtime terminals and process effects.

use peritus_conformance::{
    ProviderAttemptObservation, ProviderAttemptOutcome, ProviderCancellationObservation,
    ProviderConformanceFixture, ProviderConformanceObservation, ProviderEventKind,
    ProviderEventObservation, ProviderFailureKind, ProviderFailureObservation,
    ProviderIsolationObservation, ProviderRetryObservation, ProviderScenario,
    ProviderStreamObservation, ProviderTerminal, ProviderUsageObservation, ProviderUsageSnapshot,
    ReportText,
};
use peritus_model_protocol::{EventEnvelope, FailureCategory, ModelEvent};

use super::diagnostics::ProbeError;
use super::support::{ForeignProbe, Probe, RecoveryProbe, is_terminal};

pub(super) mod capabilities;

pub(super) fn exercise(
    fixture: &ProviderConformanceFixture,
) -> Result<ProviderConformanceObservation, ProbeError> {
    match fixture.scenario() {
        ProviderScenario::CapabilityHonesty => capabilities::observe(fixture),
        ProviderScenario::RateLimitRetryAfter | ProviderScenario::TransientRetry => {
            recovery(fixture)
        }
        scenario => observe_once(fixture, scenario),
    }
}

fn observe_once(
    fixture: &ProviderConformanceFixture,
    scenario: ProviderScenario,
) -> Result<ProviderConformanceObservation, ProbeError> {
    let probe = Probe::run(fixture)?;
    match scenario {
        ProviderScenario::OrderedDeduplication => Ok(ordered(&probe)),
        ProviderScenario::FragmentedToolCall => {
            Ok(fragmented(&probe.events, fixture.expected_tool_arguments_digest()))
        }
        ProviderScenario::MalformedPayload => {
            failure(&probe, FailureCategory::MalformedPayload, ProviderFailureKind::Malformed, 0)
        }
        ProviderScenario::IncompleteStream => {
            failure(&probe, FailureCategory::IncompleteStream, ProviderFailureKind::Incomplete, 1)
        }
        ProviderScenario::Interruption => {
            failure(&probe, FailureCategory::Transport, ProviderFailureKind::Interrupted, 1)
        }
        ProviderScenario::Cancellation => cancellation(&probe),
        ProviderScenario::AuthenticationFailure => {
            failure(&probe, FailureCategory::Authentication, ProviderFailureKind::Authentication, 0)
        }
        ProviderScenario::AmbiguousSubmission => ambiguous(&probe),
        ProviderScenario::UsageAccounting => Ok(usage(&probe.events)),
        ProviderScenario::Redaction => super::redaction::observe(&probe, fixture)
            .map_err(|_| probe_error(&probe, "verify.redaction")),
        ProviderScenario::AdapterIsolation => isolation(&probe, fixture),
        ProviderScenario::CapabilityHonesty
        | ProviderScenario::RateLimitRetryAfter
        | ProviderScenario::TransientRetry => Err(ProbeError::stage("observe.scenario")),
    }
}

fn ordered(probe: &Probe) -> ProviderConformanceObservation {
    let events = observed_events(&probe.events);
    let received = u64::try_from(events.len()).unwrap_or(u64::MAX).saturating_add(1);
    let duplicates =
        u64::try_from(probe.trace.iter().filter(|entry| entry.as_str() == "duplicate").count())
            .unwrap_or(u64::MAX);
    ProviderConformanceObservation::Stream(ProviderStreamObservation::new(
        events, received, duplicates, 1, None, None, None,
    ))
}

fn fragmented(events: &[EventEnvelope], digest: [u8; 32]) -> ProviderConformanceObservation {
    let events = observed_events(events);
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
    let received = u64::try_from(events.len()).unwrap_or(u64::MAX);
    ProviderConformanceObservation::Stream(ProviderStreamObservation::new(
        events,
        received,
        0,
        1,
        Some(digest),
        final_fragment,
        closed,
    ))
}

pub(super) fn failure(
    probe: &Probe,
    category: FailureCategory,
    kind: ProviderFailureKind,
    partial: u64,
) -> Result<ProviderConformanceObservation, ProbeError> {
    let failed_stage = if failure_category(&probe.events) != Some(category) {
        Some("verify.failure-category")
    } else if probe.auth_requests() != 1 {
        Some("verify.auth-requests")
    } else if (category == FailureCategory::Authentication && probe.turn_requests() != 0)
        || (category != FailureCategory::Authentication && probe.turn_requests() != 1)
    {
        Some("verify.turn-requests")
    } else if !probe.directory_removed {
        Some("verify.cleanup")
    } else {
        None
    };
    if let Some(stage) = failed_stage {
        return Err(probe_error(probe, stage));
    }
    Ok(ProviderConformanceObservation::Failure(ProviderFailureObservation::new(
        kind,
        ProviderTerminal::Failed,
        1,
        partial,
    )))
}

fn cancellation(probe: &Probe) -> Result<ProviderConformanceObservation, ProbeError> {
    let cancelled = matches!(
        probe.events.last().map(EventEnvelope::event),
        Some(ModelEvent::ResponseCancelled)
    );
    let spun = probe.trace.iter().any(|entry| entry == "spin");
    if !cancelled || !spun || probe.turn_requests() != 1 || !probe.directory_removed {
        return Err(probe_error(probe, "verify.cancellation"));
    }
    Ok(ProviderConformanceObservation::Cancellation(ProviderCancellationObservation::new(
        true,
        true,
        true,
        ProviderTerminal::Cancelled,
        terminal_count(&probe.events),
    )))
}

fn recovery(
    fixture: &ProviderConformanceFixture,
) -> Result<ProviderConformanceObservation, ProbeError> {
    let probe = RecoveryProbe::run(fixture)?;
    observe_recovery(&probe, fixture)
}

pub(super) fn observe_recovery(
    probe: &RecoveryProbe,
    fixture: &ProviderConformanceFixture,
) -> Result<ProviderConformanceObservation, ProbeError> {
    if failure_category(&probe.first) != Some(FailureCategory::Provider)
        || !matches!(
            probe.second.last().map(EventEnvelope::event),
            Some(ModelEvent::ResponseCompleted)
        )
        || probe.trace.iter().filter(|entry| entry.as_str() == "turn").count() != 2
        || !probe.directory_removed
    {
        return Err(probe.error("verify.recovery"));
    }
    let (outcome, delay) = match fixture.scenario() {
        ProviderScenario::RateLimitRetryAfter => {
            (ProviderAttemptOutcome::RateLimited, fixture.retry_after_millis())
        }
        ProviderScenario::TransientRetry => (
            ProviderAttemptOutcome::TransientFailure,
            u64::try_from(probe.plan.delay().as_millis())
                .map_err(|_| probe.error("recovery.delay"))?,
        ),
        _ => return Err(probe.error("recovery.scenario")),
    };
    if probe.plan.delay().as_millis() != u128::from(delay) {
        return Err(probe.error("verify.recovery-delay"));
    }
    Ok(ProviderConformanceObservation::Retry(ProviderRetryObservation::new(
        vec![
            ProviderAttemptObservation::new(1, outcome, true, 1, 0),
            ProviderAttemptObservation::new(2, ProviderAttemptOutcome::Completed, true, 1, delay),
        ],
        ProviderTerminal::Completed,
        false,
    )))
}

fn ambiguous(probe: &Probe) -> Result<ProviderConformanceObservation, ProbeError> {
    if failure_category(&probe.events) != Some(FailureCategory::AmbiguousAcceptance)
        || probe.turn_requests() != 1
        || !probe.directory_removed
    {
        return Err(probe_error(probe, "verify.ambiguous"));
    }
    Ok(ProviderConformanceObservation::Retry(ProviderRetryObservation::new(
        vec![ProviderAttemptObservation::new(1, ProviderAttemptOutcome::Ambiguous, true, 1, 0)],
        ProviderTerminal::Failed,
        true,
    )))
}

fn usage(events: &[EventEnvelope]) -> ProviderConformanceObservation {
    let snapshots = events
        .iter()
        .filter_map(|event| match event.event() {
            ModelEvent::Usage(usage) => {
                let counters = usage.counters();
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

pub(super) fn isolation(
    probe: &Probe,
    fixture: &ProviderConformanceFixture,
) -> Result<ProviderConformanceObservation, ProbeError> {
    let foreign = ForeignProbe::untouched()
        .and_then(ForeignProbe::finish)
        .map_err(|failure| failure.with_evidence(probe.evidence("selected")))?;
    let result = (|| {
        if probe.auth_requests() != 1 || probe.turn_requests() != 1 || !probe.completed() {
            return Err(ProbeError::stage("verify.isolation"));
        }
        let selected = || {
            ReportText::new(fixture.selected_adapter().to_owned())
                .map_err(|_| ProbeError::stage("isolation.adapter-name"))
        };
        Ok(ProviderConformanceObservation::Isolation(ProviderIsolationObservation::new(
            selected()?,
            selected()?,
            selected()?,
            selected()?,
            u64::try_from(foreign.requests)
                .map_err(|_| ProbeError::stage("isolation.request-count"))?,
        )))
    })();
    result.map_err(|failure| {
        failure.with_evidence(probe.evidence("selected")).with_evidence(foreign.evidence())
    })
}

fn probe_error(probe: &Probe, stage: &'static str) -> ProbeError {
    ProbeError::observed(stage, &probe.events, &probe.trace, probe.directory_removed)
}

fn observed_events(events: &[EventEnvelope]) -> Vec<ProviderEventObservation> {
    events.iter().filter_map(observed_event).collect()
}

fn observed_event(event: &EventEnvelope) -> Option<ProviderEventObservation> {
    let (kind, bytes) = match event.event() {
        ModelEvent::ResponseStarted { .. } => (ProviderEventKind::ResponseStarted, 0),
        ModelEvent::ItemStarted { .. } => (ProviderEventKind::ItemStarted, 0),
        ModelEvent::TextDelta { fragment, .. } => (ProviderEventKind::TextDelta, fragment.len()),
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
        u64::try_from(bytes).unwrap_or(u64::MAX),
    ))
}

fn failure_category(events: &[EventEnvelope]) -> Option<FailureCategory> {
    events.iter().rev().find_map(|event| match event.event() {
        ModelEvent::ResponseFailed(failure) => Some(failure.category()),
        _ => None,
    })
}

fn terminal_count(events: &[EventEnvelope]) -> u64 {
    u64::try_from(events.iter().filter(|event| is_terminal(event.event())).count())
        .unwrap_or(u64::MAX)
}
