use std::sync::{Arc, Mutex};

use peritus_conformance::{
    CaseDescriptor, CaseStatus, ConformanceFuture, ConformanceRunner, ProviderAttemptObservation,
    ProviderAttemptOutcome, ProviderCancellationObservation, ProviderCapability,
    ProviderCapabilityObservation, ProviderConformanceError, ProviderConformanceFixture,
    ProviderConformanceObservation, ProviderConformanceSubject, ProviderEventKind,
    ProviderEventObservation, ProviderFailureKind, ProviderFailureObservation,
    ProviderIsolationObservation, ProviderRedactionObservation, ProviderRetryObservation,
    ProviderScenario, ProviderStreamObservation, ProviderTerminal, ProviderUsageObservation,
    ProviderUsageSnapshot, SubjectDescriptor, SubjectFactory, SubjectFailure, SuiteStatus,
    provider_suite,
};

use super::harness::{block_on, text};

#[derive(Clone, Copy, Default)]
enum Behavior {
    #[default]
    Honest,
    CapabilityLie,
    RetryAmbiguous,
    FinalResultOnly,
}

struct ReferenceProvider {
    behavior: Behavior,
}

impl ProviderConformanceSubject for ReferenceProvider {
    fn exercise(
        &mut self,
        fixture: &ProviderConformanceFixture,
    ) -> Result<ProviderConformanceObservation, ProviderConformanceError> {
        Ok(match fixture.scenario() {
            ProviderScenario::CapabilityHonesty => capabilities(self.behavior),
            ProviderScenario::OrderedDeduplication => ordered_stream(self.behavior),
            ProviderScenario::FragmentedToolCall => {
                fragmented_stream(fixture.expected_tool_arguments_digest())
            }
            ProviderScenario::MalformedPayload => failed(ProviderFailureKind::Malformed, 0),
            ProviderScenario::IncompleteStream => failed(ProviderFailureKind::Incomplete, 2),
            ProviderScenario::Interruption => failed(ProviderFailureKind::Interrupted, 3),
            ProviderScenario::Cancellation => {
                ProviderConformanceObservation::Cancellation(ProviderCancellationObservation::new(
                    true,
                    true,
                    true,
                    ProviderTerminal::Cancelled,
                    1,
                ))
            }
            ProviderScenario::AuthenticationFailure => {
                failed(ProviderFailureKind::Authentication, 0)
            }
            ProviderScenario::RateLimitRetryAfter => rate_limit(fixture.retry_after_millis()),
            ProviderScenario::TransientRetry => transient(),
            ProviderScenario::AmbiguousSubmission => ambiguous(self.behavior),
            ProviderScenario::UsageAccounting => usage(),
            ProviderScenario::Redaction => {
                ProviderConformanceObservation::Redaction(ProviderRedactionObservation::new(
                    4,
                    vec![text("credential=[redacted]"), text("payload-bytes=48")],
                ))
            }
            ProviderScenario::AdapterIsolation => {
                ProviderConformanceObservation::Isolation(ProviderIsolationObservation::new(
                    text(fixture.selected_adapter()),
                    text(fixture.selected_adapter()),
                    text(fixture.selected_adapter()),
                    text(fixture.selected_adapter()),
                    0,
                ))
            }
        })
    }
}

fn capabilities(behavior: Behavior) -> ProviderConformanceObservation {
    let advertised = vec![
        ProviderCapability::Streaming,
        ProviderCapability::ToolCalls,
        ProviderCapability::UsageDetail,
    ];
    let succeeded = if matches!(behavior, Behavior::CapabilityLie) {
        vec![ProviderCapability::Streaming, ProviderCapability::UsageDetail]
    } else {
        advertised.clone()
    };
    ProviderConformanceObservation::Capabilities(ProviderCapabilityObservation::new(
        advertised.clone(),
        succeeded,
        vec![ProviderCapability::AudioInput],
        advertised,
        3,
    ))
}

fn event(sequence: u64, kind: ProviderEventKind, fragment_bytes: u64) -> ProviderEventObservation {
    ProviderEventObservation::new(
        sequence,
        Some(sequence),
        [u8::try_from(sequence).expect("small sequence"); 32],
        [u8::try_from(sequence + 16).expect("small sequence"); 32],
        kind,
        fragment_bytes,
    )
}

fn ordered_stream(behavior: Behavior) -> ProviderConformanceObservation {
    let events = vec![
        event(1, ProviderEventKind::ResponseStarted, 0),
        event(2, ProviderEventKind::ItemStarted, 0),
        event(3, ProviderEventKind::TextDelta, 5),
        event(4, ProviderEventKind::Finish, 0),
        event(5, ProviderEventKind::ResponseCompleted, 0),
    ];
    let observation = if matches!(behavior, Behavior::FinalResultOnly) {
        ProviderStreamObservation::new(events, 5, 0, 1, None, None, None)
            .without_provider_event_deduplication()
    } else {
        ProviderStreamObservation::new(events, 6, 1, 1, None, None, None)
    };
    ProviderConformanceObservation::Stream(observation)
}

fn fragmented_stream(digest: [u8; 32]) -> ProviderConformanceObservation {
    ProviderConformanceObservation::Stream(ProviderStreamObservation::new(
        vec![
            event(1, ProviderEventKind::ResponseStarted, 0),
            event(2, ProviderEventKind::ItemStarted, 0),
            event(3, ProviderEventKind::ToolCallStarted, 0),
            event(4, ProviderEventKind::ToolArgumentDelta, 9),
            event(5, ProviderEventKind::ToolArgumentDelta, 7),
            event(6, ProviderEventKind::ItemCompleted, 0),
            event(7, ProviderEventKind::Finish, 0),
            event(8, ProviderEventKind::ResponseCompleted, 0),
        ],
        8,
        0,
        1,
        Some(digest),
        Some(5),
        Some(6),
    ))
}

const fn failed(kind: ProviderFailureKind, partial_events: u64) -> ProviderConformanceObservation {
    ProviderConformanceObservation::Failure(ProviderFailureObservation::new(
        kind,
        ProviderTerminal::Failed,
        1,
        partial_events,
    ))
}

fn rate_limit(delay: u64) -> ProviderConformanceObservation {
    ProviderConformanceObservation::Retry(ProviderRetryObservation::new(
        vec![
            ProviderAttemptObservation::new(1, ProviderAttemptOutcome::RateLimited, true, 0, 0),
            ProviderAttemptObservation::new(2, ProviderAttemptOutcome::Completed, true, 5, delay),
        ],
        ProviderTerminal::Completed,
        false,
    ))
}

fn transient() -> ProviderConformanceObservation {
    ProviderConformanceObservation::Retry(ProviderRetryObservation::new(
        vec![
            ProviderAttemptObservation::new(
                1,
                ProviderAttemptOutcome::TransientFailure,
                false,
                0,
                0,
            ),
            ProviderAttemptObservation::new(2, ProviderAttemptOutcome::Completed, true, 5, 100),
        ],
        ProviderTerminal::Completed,
        false,
    ))
}

fn ambiguous(behavior: Behavior) -> ProviderConformanceObservation {
    let mut attempts =
        vec![ProviderAttemptObservation::new(1, ProviderAttemptOutcome::Ambiguous, true, 0, 0)];
    if matches!(behavior, Behavior::RetryAmbiguous) {
        attempts.push(ProviderAttemptObservation::new(
            2,
            ProviderAttemptOutcome::Completed,
            true,
            5,
            100,
        ));
    }
    ProviderConformanceObservation::Retry(ProviderRetryObservation::new(
        attempts,
        ProviderTerminal::Failed,
        true,
    ))
}

fn usage() -> ProviderConformanceObservation {
    ProviderConformanceObservation::Usage(ProviderUsageObservation::new(vec![
        ProviderUsageSnapshot::new(Some(10), Some(2), Some(1), Some(11)),
        ProviderUsageSnapshot::new(Some(10), Some(2), Some(3), Some(13)),
    ]))
}

#[derive(Clone, Copy, Default)]
struct Counts {
    created: usize,
    torn_down: usize,
}

struct Factory {
    descriptor: SubjectDescriptor,
    counts: Arc<Mutex<Counts>>,
    behavior: Behavior,
}

impl Factory {
    fn new(behavior: Behavior) -> Self {
        Self {
            descriptor: SubjectDescriptor::new(
                text("provider-reference"),
                text("A2 provider oracle"),
            ),
            counts: Arc::new(Mutex::new(Counts::default())),
            behavior,
        }
    }
}

impl SubjectFactory<ReferenceProvider> for Factory {
    fn descriptor(&self) -> &SubjectDescriptor {
        &self.descriptor
    }

    fn create<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
    ) -> ConformanceFuture<'a, Result<ReferenceProvider, SubjectFailure>> {
        self.counts.lock().expect("counts lock").created += 1;
        let behavior = self.behavior;
        Box::pin(async move { Ok(ReferenceProvider { behavior }) })
    }

    fn teardown<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
        _subject: ReferenceProvider,
    ) -> ConformanceFuture<'a, Result<(), SubjectFailure>> {
        self.counts.lock().expect("counts lock").torn_down += 1;
        Box::pin(async { Ok(()) })
    }
}

#[test]
fn provider_catalog_runs_fourteen_cases_with_fresh_subjects() {
    let factory = Factory::new(Behavior::Honest);
    let report = block_on(ConformanceRunner::run(&provider_suite::<ReferenceProvider>(), &factory));
    assert_eq!(report.status(), SuiteStatus::Passed, "{report:?}");
    assert_eq!(report.summary().total(), 14);
    let counts = *factory.counts.lock().expect("counts lock");
    assert_eq!((counts.created, counts.torn_down), (14, 14));
}

#[test]
fn provider_catalog_reports_capability_lies_and_ambiguous_retries() {
    for (behavior, expected) in [
        (Behavior::CapabilityLie, "peritus.provider.capability-honesty"),
        (Behavior::RetryAmbiguous, "peritus.provider.ambiguous-submission"),
    ] {
        let factory = Factory::new(behavior);
        let report =
            block_on(ConformanceRunner::run(&provider_suite::<ReferenceProvider>(), &factory));
        let failures = report
            .cases()
            .iter()
            .filter(|case| case.status() == CaseStatus::Failed)
            .map(|case| case.descriptor().id().as_str())
            .collect::<Vec<_>>();
        assert_eq!(failures, [expected]);
    }
}

#[test]
fn provider_catalog_accepts_explicit_final_result_ordering_without_synthetic_duplicates() {
    let factory = Factory::new(Behavior::FinalResultOnly);
    let report = block_on(ConformanceRunner::run(&provider_suite::<ReferenceProvider>(), &factory));
    assert_eq!(report.status(), SuiteStatus::Passed, "{report:?}");
}
