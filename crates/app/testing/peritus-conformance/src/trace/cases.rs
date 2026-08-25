//! Executable C7 trace and telemetry conformance cases.

use super::{TraceConformanceFixture, TraceConformanceSubject, TraceScenario};
use crate::{
    AssertionFailure, CaseDescriptor, CaseId, CaseResult, ConformanceCase, ConformanceFuture,
    FailureCode, Observation, ObservationId, ObservationValue, ReportText, StaticSuite,
    SuiteDescriptor, SuiteId,
};

struct TraceCase {
    descriptor: CaseDescriptor,
    scenario: TraceScenario,
}

impl<S: TraceConformanceSubject> ConformanceCase<S> for TraceCase {
    fn descriptor(&self) -> &CaseDescriptor {
        &self.descriptor
    }

    fn run<'a>(&'a self, subject: &'a mut S) -> ConformanceFuture<'a, CaseResult> {
        Box::pin(async move { result(subject, self.scenario) })
    }
}

/// Returns the complete runtime-neutral C7 conformance suite.
#[must_use]
pub fn trace_suite<S: TraceConformanceSubject + 'static>() -> StaticSuite<S> {
    use TraceScenario as T;
    StaticSuite::new(
        SuiteDescriptor::new(
            SuiteId::catalog("peritus.trace"),
            ReportText::literal(
                "C7 causality, redaction, bounded export, replay, and non-authority contract",
            ),
        ),
        vec![
            boxed(
                "backpressure",
                "Backpressure policies account for every offered item",
                T::Backpressure,
            ),
            boxed(
                "bounded-load",
                "Queue occupancy and drops remain within configured bounds",
                T::BoundedLoad,
            ),
            boxed(
                "causal-integrity",
                "Parents, bindings, and event order remain exact",
                T::CausalIntegrity,
            ),
            boxed(
                "duplicate-conflict",
                "Changed duplicates fail integrity validation",
                T::DuplicateConflict,
            ),
            boxed(
                "durable-replay",
                "C0 replay and shadow rebuild are byte-equivalent",
                T::DurableReplay,
            ),
            boxed(
                "exporter-failure",
                "Exporter failure retains the exact pending batch",
                T::ExporterFailure,
            ),
            boxed("non-authority", "Telemetry cannot mutate authoritative state", T::NonAuthority),
            boxed(
                "redaction-leakage",
                "Sensitive canaries never reach default surfaces",
                T::RedactionLeakage,
            ),
            boxed(
                "shutdown-recovery",
                "Bounded shutdown and restart recover pending work",
                T::ShutdownRecovery,
            ),
        ],
    )
}

fn boxed<S: TraceConformanceSubject + 'static>(
    suffix: &'static str,
    summary: &'static str,
    scenario: TraceScenario,
) -> Box<dyn ConformanceCase<S>> {
    Box::new(TraceCase {
        descriptor: CaseDescriptor::new(
            CaseId::new(format!("peritus.trace.{suffix}")).expect("static trace case identifier"),
            ReportText::literal(summary),
        ),
        scenario,
    })
}

fn result<S: TraceConformanceSubject>(subject: &mut S, scenario: TraceScenario) -> CaseResult {
    let fixture = TraceConformanceFixture::new(scenario);
    let Ok(observed) = subject.exercise(&fixture) else {
        return failed(scenario, 0, 0, false);
    };
    let bounded = observed.peak_buffered <= fixture.queue_capacity();
    let accounted = observed
        .exported
        .checked_add(observed.dropped)
        .is_some_and(|settled| settled <= observed.accepted)
        && observed.accounting_exact;
    let common = bounded
        && accounted
        && observed.causal_integrity
        && observed.replay_equivalent
        && observed.leakage_free
        && observed.non_authoritative;
    let exact = common
        && match scenario {
            TraceScenario::CausalIntegrity => observed.causal_integrity,
            TraceScenario::RedactionLeakage => observed.leakage_free,
            TraceScenario::BoundedLoad | TraceScenario::Backpressure => bounded && accounted,
            TraceScenario::ExporterFailure => observed.failure_retained,
            TraceScenario::DurableReplay => observed.replay_equivalent,
            TraceScenario::DuplicateConflict => observed.duplicate_integrity,
            TraceScenario::ShutdownRecovery => observed.recovery_exact && observed.failure_retained,
            TraceScenario::NonAuthority => observed.non_authoritative,
        };
    if exact {
        CaseResult::passed(observations(observed.accepted, observed.dropped, true))
    } else {
        failed(scenario, observed.accepted, observed.dropped, bounded && accounted)
    }
}

fn failed(scenario: TraceScenario, accepted: u64, dropped: u64, exact: bool) -> CaseResult {
    CaseResult::failed(observations(accepted, dropped, exact), assertion(scenario))
}

fn observations(accepted: u64, dropped: u64, exact: bool) -> Vec<Observation> {
    vec![
        Observation::new(ObservationId::catalog("accepted"), ObservationValue::Unsigned(accepted)),
        Observation::new(ObservationId::catalog("dropped"), ObservationValue::Unsigned(dropped)),
        Observation::new(ObservationId::catalog("exact"), ObservationValue::Boolean(exact)),
    ]
}

fn assertion(scenario: TraceScenario) -> AssertionFailure {
    let number = match scenario {
        TraceScenario::CausalIntegrity => "001",
        TraceScenario::RedactionLeakage => "002",
        TraceScenario::BoundedLoad => "003",
        TraceScenario::ExporterFailure => "004",
        TraceScenario::DurableReplay => "005",
        TraceScenario::DuplicateConflict => "006",
        TraceScenario::Backpressure => "007",
        TraceScenario::ShutdownRecovery => "008",
        TraceScenario::NonAuthority => "009",
    };
    AssertionFailure::new(
        FailureCode::new(format!("PERITUS-TRACE-CONFORMANCE-{number}"))
            .expect("static trace failure code"),
        ReportText::literal("C7 direct observations violated the selected trace contract"),
        None,
        None,
    )
}
