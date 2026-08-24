//! Direct validation of provider-subject observations.

use std::collections::BTreeSet;

use super::super::fixtures::fixture;
use super::super::{
    ProviderAttemptOutcome, ProviderConformanceObservation, ProviderConformanceSubject,
    ProviderEventKind, ProviderFailureKind, ProviderScenario, ProviderTerminal,
    ProviderUsageSnapshot,
};

pub(super) fn exercise<S: ProviderConformanceSubject>(
    subject: &mut S,
    scenario: ProviderScenario,
) -> bool {
    let request = fixture(scenario);
    let Ok(observed) = subject.exercise(&request) else {
        return false;
    };
    match scenario {
        ProviderScenario::CapabilityHonesty => capability(observed),
        ProviderScenario::OrderedDeduplication => ordered(observed),
        ProviderScenario::FragmentedToolCall => {
            fragmented(observed, request.expected_tool_arguments_digest())
        }
        ProviderScenario::MalformedPayload => {
            failure(&observed, ProviderFailureKind::Malformed, false)
        }
        ProviderScenario::IncompleteStream => {
            failure(&observed, ProviderFailureKind::Incomplete, true)
        }
        ProviderScenario::Interruption => {
            failure(&observed, ProviderFailureKind::Interrupted, true)
        }
        ProviderScenario::Cancellation => cancellation(&observed),
        ProviderScenario::AuthenticationFailure => {
            failure(&observed, ProviderFailureKind::Authentication, false)
        }
        ProviderScenario::RateLimitRetryAfter => rate_limit(observed, request.retry_after_millis()),
        ProviderScenario::TransientRetry => transient(observed, request.max_retry_delay_millis()),
        ProviderScenario::AmbiguousSubmission => ambiguous(observed),
        ProviderScenario::UsageAccounting => usage(observed),
        ProviderScenario::Redaction => redaction(observed, request.canary()),
        ProviderScenario::AdapterIsolation => isolation(observed, request.selected_adapter()),
    }
}

fn capability(observed: ProviderConformanceObservation) -> bool {
    let ProviderConformanceObservation::Capabilities(observed) = observed else {
        return false;
    };
    let advertised = observed.advertised();
    let succeeded = observed.succeeded();
    let rejected = observed.rejected_before_transport();
    !advertised.is_empty()
        && !rejected.is_empty()
        && unique(advertised)
        && unique(rejected)
        && same_set(advertised, succeeded)
        && observed.encoded().iter().all(|feature| advertised.contains(feature))
        && rejected.iter().all(|feature| !advertised.contains(feature))
        && usize::try_from(observed.transport_requests()).ok() == Some(succeeded.len())
}

fn ordered(observed: ProviderConformanceObservation) -> bool {
    let ProviderConformanceObservation::Stream(observed) = observed else {
        return false;
    };
    let events = observed.events();
    let emitted = u64::try_from(events.len()).ok();
    let deduplication_is_exact = if observed.provider_deduplication_applicable() {
        observed.duplicate_events() > 0
            && emitted.and_then(|count| count.checked_add(observed.duplicate_events()))
                == Some(observed.received_events())
    } else {
        observed.duplicate_events() == 0 && emitted == Some(observed.received_events())
    };
    events.first().is_some_and(|event| event.kind() == ProviderEventKind::ResponseStarted)
        && events.last().is_some_and(|event| event.kind() == ProviderEventKind::ResponseCompleted)
        && strictly_increasing(events.iter().map(|event| event.sequence()))
        && deduplication_is_exact
        && observed.terminal_count() == 1
}

fn fragmented(observed: ProviderConformanceObservation, expected: [u8; 32]) -> bool {
    let ProviderConformanceObservation::Stream(observed) = observed else {
        return false;
    };
    let fragments = observed
        .events()
        .iter()
        .filter(|event| event.kind() == ProviderEventKind::ToolArgumentDelta)
        .collect::<Vec<_>>();
    fragments.len() >= 2
        && fragments.iter().all(|event| event.fragment_bytes() > 0)
        && observed.completed_tool_digest() == Some(expected)
        && observed
            .final_fragment_sequence()
            .zip(observed.tool_closed_sequence())
            .is_some_and(|(fragment, closed)| fragment < closed)
        && observed.terminal_count() == 1
}

fn failure(
    observed: &ProviderConformanceObservation,
    expected: ProviderFailureKind,
    requires_partial: bool,
) -> bool {
    let ProviderConformanceObservation::Failure(observed) = observed else {
        return false;
    };
    observed.kind() == expected
        && observed.terminal() == ProviderTerminal::Failed
        && observed.transport_requests() == 1
        && (observed.partial_events() > 0) == requires_partial
}

fn cancellation(observed: &ProviderConformanceObservation) -> bool {
    let ProviderConformanceObservation::Cancellation(observed) = observed else {
        return false;
    };
    observed.control_observed()
        && observed.pending_work_interrupted()
        && observed.worker_joined()
        && observed.terminal() == ProviderTerminal::Cancelled
        && observed.terminal_count() == 1
}

fn rate_limit(observed: ProviderConformanceObservation, retry_after: u64) -> bool {
    let ProviderConformanceObservation::Retry(observed) = observed else {
        return false;
    };
    matches!(
        observed.attempts(),
        [first, second]
            if first.ordinal() == 1
                && first.outcome() == ProviderAttemptOutcome::RateLimited
                && first.request_bytes_sent()
                && first.delay_before_millis() == 0
                && second.ordinal() == 2
                && second.outcome() == ProviderAttemptOutcome::Completed
                && second.delay_before_millis() == retry_after
    ) && observed.terminal() == ProviderTerminal::Completed
        && !observed.ambiguous()
}

fn transient(observed: ProviderConformanceObservation, maximum_delay: u64) -> bool {
    let ProviderConformanceObservation::Retry(observed) = observed else {
        return false;
    };
    matches!(
        observed.attempts(),
        [first, second]
            if first.ordinal() == 1
                && first.outcome() == ProviderAttemptOutcome::TransientFailure
                && second.ordinal() == 2
                && second.outcome() == ProviderAttemptOutcome::Completed
                && second.delay_before_millis() > 0
                && second.delay_before_millis() <= maximum_delay
    ) && observed.terminal() == ProviderTerminal::Completed
        && !observed.ambiguous()
}

fn ambiguous(observed: ProviderConformanceObservation) -> bool {
    let ProviderConformanceObservation::Retry(observed) = observed else {
        return false;
    };
    matches!(
        observed.attempts(),
        [attempt]
            if attempt.ordinal() == 1
                && attempt.outcome() == ProviderAttemptOutcome::Ambiguous
                && attempt.request_bytes_sent()
    ) && observed.terminal() == ProviderTerminal::Failed
        && observed.ambiguous()
}

fn usage(observed: ProviderConformanceObservation) -> bool {
    let ProviderConformanceObservation::Usage(observed) = observed else {
        return false;
    };
    let snapshots = observed.snapshots();
    !snapshots.is_empty()
        && snapshots.windows(2).all(|pair| usage_monotonic(pair[0], pair[1]))
        && snapshots.last().is_some_and(usage_consistent)
}

fn redaction(observed: ProviderConformanceObservation, canary: &str) -> bool {
    let ProviderConformanceObservation::Redaction(observed) = observed else {
        return false;
    };
    observed.sensitive_inputs() >= 4
        && !observed.surfaces().is_empty()
        && observed.surfaces().iter().all(|surface| !surface.as_str().contains(canary))
}

fn isolation(observed: ProviderConformanceObservation, selected: &str) -> bool {
    let ProviderConformanceObservation::Isolation(observed) = observed else {
        return false;
    };
    observed.configured_adapter().as_str() == selected
        && observed.request_adapter().as_str() == selected
        && observed.credential_adapter().as_str() == selected
        && observed.transport_adapter().as_str() == selected
        && observed.foreign_transport_requests() == 0
}

fn unique<T: Ord + Copy>(values: &[T]) -> bool {
    values.iter().copied().collect::<BTreeSet<_>>().len() == values.len()
}

fn same_set<T: Ord + Copy>(left: &[T], right: &[T]) -> bool {
    unique(left)
        && unique(right)
        && left.iter().copied().collect::<BTreeSet<_>>()
            == right.iter().copied().collect::<BTreeSet<_>>()
}

fn strictly_increasing(values: impl Iterator<Item = u64>) -> bool {
    let mut prior = None;
    for value in values {
        if value == 0 || prior.is_some_and(|previous| previous >= value) {
            return false;
        }
        prior = Some(value);
    }
    prior.is_some()
}

const fn usage_monotonic(previous: ProviderUsageSnapshot, current: ProviderUsageSnapshot) -> bool {
    monotonic(previous.input(), current.input())
        && monotonic(previous.cached_input(), current.cached_input())
        && monotonic(previous.output(), current.output())
        && monotonic(previous.total(), current.total())
}

const fn monotonic(previous: Option<u64>, current: Option<u64>) -> bool {
    match (previous, current) {
        (Some(left), Some(right)) => left <= right,
        (Some(_), None) => false,
        (None, _) => true,
    }
}

fn usage_consistent(snapshot: &ProviderUsageSnapshot) -> bool {
    match (snapshot.input(), snapshot.output(), snapshot.total()) {
        (Some(input), Some(output), Some(total)) => input.checked_add(output) == Some(total),
        _ => true,
    }
}
