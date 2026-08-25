//! Executable D2 review-engine conformance cases.

use super::{ReviewConformanceFixture, ReviewConformanceSubject, ReviewScenario, ReviewTerminal};
use crate::{
    AssertionFailure, CaseDescriptor, CaseId, CaseResult, ConformanceCase, ConformanceFuture,
    FailureCode, Observation, ObservationId, ObservationValue, ReportText, StaticSuite,
    SuiteDescriptor, SuiteId,
};

struct ReviewCase {
    descriptor: CaseDescriptor,
    scenario: ReviewScenario,
}

impl<S: ReviewConformanceSubject> ConformanceCase<S> for ReviewCase {
    fn descriptor(&self) -> &CaseDescriptor {
        &self.descriptor
    }

    fn run<'a>(&'a self, subject: &'a mut S) -> ConformanceFuture<'a, CaseResult> {
        Box::pin(async move { result(subject, self.scenario) })
    }
}

/// Returns the complete runtime-neutral D2 conformance suite.
#[must_use]
pub fn review_suite<S: ReviewConformanceSubject + 'static>() -> StaticSuite<S> {
    use ReviewScenario as R;
    StaticSuite::new(
        SuiteDescriptor::new(
            SuiteId::catalog("peritus.review"),
            ReportText::literal(
                "D2 lifecycle, quorum, independence, conservation, authority, and replay contract",
            ),
        ),
        vec![
            boxed("independence", "Every independence dimension is enforced", R::Independence),
            boxed("lifecycle", "The closed review and finding lifecycle completes", R::Lifecycle),
            boxed(
                "malformed-submission",
                "Malformed structured data cannot count or complete",
                R::MalformedSubmission,
            ),
            boxed("oscillation", "Review oscillation terminates truthfully", R::Oscillation),
            boxed("quorum", "Review count and category quorum are exact", R::Quorum),
            boxed(
                "reconciliation",
                "Duplicate reconciliation retains complete provenance",
                R::Reconciliation,
            ),
            boxed("resolution", "Finding closure requires reviewer confirmation", R::Resolution),
            boxed("restart", "Restart and command replay are exact", R::Restart),
            boxed("stale-revision", "Stale evidence cannot become current", R::StaleRevision),
            boxed("waiver", "Waiver closure requires external authority", R::Waiver),
        ],
    )
}

fn boxed<S: ReviewConformanceSubject + 'static>(
    suffix: &'static str,
    summary: &'static str,
    scenario: ReviewScenario,
) -> Box<dyn ConformanceCase<S>> {
    Box::new(ReviewCase {
        descriptor: CaseDescriptor::new(
            CaseId::new(format!("peritus.review.{suffix}")).expect("static review case identifier"),
            ReportText::literal(summary),
        ),
        scenario,
    })
}

fn result<S: ReviewConformanceSubject>(subject: &mut S, scenario: ReviewScenario) -> CaseResult {
    let fixture = ReviewConformanceFixture::new(scenario);
    let Ok(observed) = subject.exercise(&fixture) else {
        return failed(scenario, 0, 0, false);
    };
    let bounded = observed.cycles <= fixture.maximum_cycles()
        && observed.findings <= fixture.maximum_findings();
    let common = bounded
        && observed.revision_exact
        && observed.findings_conserved
        && observed.no_implicit_success;
    let completed = observed.terminal == ReviewTerminal::Completed;
    let exact = common
        && match scenario {
            ReviewScenario::Lifecycle => {
                completed && observed.quorum_complete && observed.reviewer_confirmed
            }
            ReviewScenario::Quorum => completed && observed.quorum_complete,
            ReviewScenario::Independence => completed && observed.independence_complete,
            ReviewScenario::Reconciliation => completed && observed.provenance_retained,
            ReviewScenario::StaleRevision => !completed && observed.stale_rejected,
            ReviewScenario::Resolution => completed && observed.reviewer_confirmed,
            ReviewScenario::Waiver => completed && observed.waiver_external,
            ReviewScenario::Restart => observed.replay_equivalent && observed.idempotent_recovery,
            ReviewScenario::Oscillation => !completed && observed.oscillation_truthful,
            ReviewScenario::MalformedSubmission => !completed && observed.malformed_rejected,
        };
    if exact {
        CaseResult::passed(observations(observed.cycles, observed.findings, true))
    } else {
        failed(scenario, observed.cycles, observed.findings, bounded)
    }
}

fn failed(scenario: ReviewScenario, cycles: u16, findings: u16, exact: bool) -> CaseResult {
    CaseResult::failed(observations(cycles, findings, exact), assertion(scenario))
}

fn observations(cycles: u16, findings: u16, exact: bool) -> Vec<Observation> {
    vec![
        Observation::new(
            ObservationId::catalog("cycles"),
            ObservationValue::Unsigned(u64::from(cycles)),
        ),
        Observation::new(
            ObservationId::catalog("findings"),
            ObservationValue::Unsigned(u64::from(findings)),
        ),
        Observation::new(ObservationId::catalog("exact"), ObservationValue::Boolean(exact)),
    ]
}

fn assertion(scenario: ReviewScenario) -> AssertionFailure {
    let number = match scenario {
        ReviewScenario::Lifecycle => "001",
        ReviewScenario::Quorum => "002",
        ReviewScenario::Independence => "003",
        ReviewScenario::Reconciliation => "004",
        ReviewScenario::StaleRevision => "005",
        ReviewScenario::Resolution => "006",
        ReviewScenario::Waiver => "007",
        ReviewScenario::Restart => "008",
        ReviewScenario::Oscillation => "009",
        ReviewScenario::MalformedSubmission => "010",
    };
    AssertionFailure::new(
        FailureCode::new(format!("PERITUS-REVIEW-CONFORMANCE-{number}"))
            .expect("static review failure code"),
        ReportText::literal("D2 direct observations violated the selected review contract"),
        None,
        None,
    )
}
