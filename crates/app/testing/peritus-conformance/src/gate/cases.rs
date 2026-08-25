//! Executable D1 gate-engine conformance cases.

use super::{GateConformanceFixture, GateConformanceSubject, GateScenario, GateTerminal};
use crate::{
    AssertionFailure, CaseDescriptor, CaseId, CaseResult, ConformanceCase, ConformanceFuture,
    FailureCode, Observation, ObservationId, ObservationValue, ReportText, StaticSuite,
    SuiteDescriptor, SuiteId,
};

struct GateCase {
    descriptor: CaseDescriptor,
    scenario: GateScenario,
}

impl<S: GateConformanceSubject> ConformanceCase<S> for GateCase {
    fn descriptor(&self) -> &CaseDescriptor {
        &self.descriptor
    }

    fn run<'a>(&'a self, subject: &'a mut S) -> ConformanceFuture<'a, CaseResult> {
        Box::pin(async move { result(subject, self.scenario) })
    }
}

/// Returns the complete runtime-neutral D1 conformance suite.
#[must_use]
pub fn gate_suite<S: GateConformanceSubject + 'static>() -> StaticSuite<S> {
    use GateScenario as G;
    StaticSuite::new(
        SuiteDescriptor::new(
            SuiteId::catalog("peritus.gate"),
            ReportText::literal(
                "D1 dependency, quality authority, freshness, evidence, and replay contract",
            ),
        ),
        vec![
            boxed(
                "artifact-evidence",
                "Passing evidence is complete and provenance-bound",
                G::ArtifactEvidence,
            ),
            boxed(
                "cancellation",
                "Cancellation settles active gates without success",
                G::Cancellation,
            ),
            boxed(
                "clean-snapshot",
                "Dispatch requires the exact clean candidate snapshot",
                G::CleanSnapshot,
            ),
            boxed(
                "crash-recovery",
                "Restart is replay-equivalent and does not redispatch uncertainty",
                G::CrashRecovery,
            ),
            boxed(
                "deterministic-aggregation",
                "Terminal aggregation is canonically ordered",
                G::DeterministicAggregation,
            ),
            boxed(
                "failed-prerequisite",
                "Failed prerequisites block dependent dispatch",
                G::FailedPrerequisite,
            ),
            boxed(
                "inspect-edit-run-test",
                "The coding flow reaches fresh gate truth",
                G::InspectEditRunTest,
            ),
            boxed("malformed-parser", "Malformed parser output cannot pass", G::MalformedParser),
            boxed("retry-bound", "Retry legality and attempt ceilings are exact", G::RetryBound),
            boxed(
                "stale-revision",
                "Stale evidence cannot satisfy the current revision",
                G::StaleRevision,
            ),
        ],
    )
}

fn boxed<S: GateConformanceSubject + 'static>(
    suffix: &'static str,
    summary: &'static str,
    scenario: GateScenario,
) -> Box<dyn ConformanceCase<S>> {
    let id = format!("peritus.gate.{suffix}");
    Box::new(GateCase {
        descriptor: CaseDescriptor::new(
            CaseId::new(id).expect("static gate case identifier"),
            ReportText::literal(summary),
        ),
        scenario,
    })
}

fn result<S: GateConformanceSubject>(subject: &mut S, scenario: GateScenario) -> CaseResult {
    let fixture = GateConformanceFixture::new(scenario);
    let Ok(observed) = subject.exercise(&fixture) else {
        return failed(scenario, 0, 0, false);
    };
    let bounded = observed.peak_attempt <= fixture.maximum_attempts()
        && observed.dispatches <= fixture.maximum_dispatches();
    let common = bounded
        && observed.dependencies_ordered
        && observed.revision_exact
        && observed.no_implicit_success
        && observed.authority_before_effect
        && observed.replay_equivalent;
    let exact = common
        && match scenario {
            GateScenario::InspectEditRunTest => {
                observed.terminal == GateTerminal::Passed
                    && observed.clean_snapshot
                    && observed.evidence_complete
            }
            GateScenario::FailedPrerequisite => {
                observed.terminal == GateTerminal::Blocked && observed.stable_aggregation
            }
            GateScenario::MalformedParser | GateScenario::StaleRevision => {
                observed.terminal != GateTerminal::Passed && observed.no_implicit_success
            }
            GateScenario::Cancellation => observed.terminal == GateTerminal::Cancelled,
            GateScenario::CrashRecovery => observed.idempotent_recovery,
            GateScenario::CleanSnapshot => observed.clean_snapshot,
            GateScenario::RetryBound => observed.idempotent_recovery && bounded,
            GateScenario::ArtifactEvidence => observed.evidence_complete,
            GateScenario::DeterministicAggregation => observed.stable_aggregation,
        };
    if exact {
        CaseResult::passed(observations(observed.peak_attempt, observed.dispatches, true))
    } else {
        failed(scenario, observed.peak_attempt, observed.dispatches, bounded)
    }
}

fn failed(scenario: GateScenario, attempts: u16, dispatches: u16, bounded: bool) -> CaseResult {
    CaseResult::failed(observations(attempts, dispatches, bounded), assertion(scenario))
}

fn observations(attempts: u16, dispatches: u16, exact: bool) -> Vec<Observation> {
    vec![
        Observation::new(
            ObservationId::catalog("peak-attempt"),
            ObservationValue::Unsigned(u64::from(attempts)),
        ),
        Observation::new(
            ObservationId::catalog("dispatches"),
            ObservationValue::Unsigned(u64::from(dispatches)),
        ),
        Observation::new(ObservationId::catalog("exact"), ObservationValue::Boolean(exact)),
    ]
}

fn assertion(scenario: GateScenario) -> AssertionFailure {
    let number = match scenario {
        GateScenario::InspectEditRunTest => "001",
        GateScenario::FailedPrerequisite => "002",
        GateScenario::MalformedParser => "003",
        GateScenario::StaleRevision => "004",
        GateScenario::Cancellation => "005",
        GateScenario::CrashRecovery => "006",
        GateScenario::CleanSnapshot => "007",
        GateScenario::RetryBound => "008",
        GateScenario::ArtifactEvidence => "009",
        GateScenario::DeterministicAggregation => "010",
    };
    AssertionFailure::new(
        FailureCode::new(format!("PERITUS-GATE-CONFORMANCE-{number}"))
            .expect("static gate failure code"),
        ReportText::literal("D1 direct observations violated the selected gate contract"),
        None,
        None,
    )
}
