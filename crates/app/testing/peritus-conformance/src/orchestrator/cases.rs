//! Executable E0 actor orchestration conformance cases.

use super::{
    OrchestratorConformanceFixture, OrchestratorConformanceSubject, OrchestratorScenario,
    OrchestratorTerminal,
};
use crate::{
    AssertionFailure, CaseDescriptor, CaseId, CaseResult, ConformanceCase, ConformanceFuture,
    FailureCode, Observation, ObservationId, ObservationValue, ReportText, StaticSuite,
    SuiteDescriptor, SuiteId,
};

struct OrchestratorCase {
    descriptor: CaseDescriptor,
    scenario: OrchestratorScenario,
}

impl<S: OrchestratorConformanceSubject> ConformanceCase<S> for OrchestratorCase {
    fn descriptor(&self) -> &CaseDescriptor {
        &self.descriptor
    }

    fn run<'a>(&'a self, subject: &'a mut S) -> ConformanceFuture<'a, CaseResult> {
        Box::pin(async move { result(subject, self.scenario) })
    }
}

/// Returns the complete runtime-neutral E0 conformance suite.
#[must_use]
pub fn orchestrator_suite<S: OrchestratorConformanceSubject + 'static>() -> StaticSuite<S> {
    use OrchestratorScenario as O;
    StaticSuite::new(
        SuiteDescriptor::new(
            SuiteId::catalog("peritus.orchestrator"),
            ReportText::literal(
                "E0 phase, role, freshness, durability, cancellation, and acceptance contract",
            ),
        ),
        vec![
            boxed("cancellation", "Cancellation dominates late child success", O::Cancellation),
            boxed("fix-cycle", "Findings take the sole bounded fixer loop", O::FixCycle),
            boxed("happy-path", "Acceptance follows the complete authority chain", O::HappyPath),
            boxed(
                "limit-exhaustion",
                "Independent completion bounds terminate truthfully",
                O::LimitExhaustion,
            ),
            boxed(
                "malformed-protocol",
                "Malformed protocol input stays inert and rejected",
                O::MalformedProtocol,
            ),
            boxed("panic", "Subject panic is contained as failure", O::PanicContainment),
            boxed("pause-resume", "Pause resumes only after reconciliation", O::PauseResume),
            boxed("restart", "Replay and retry recover without duplicate effects", O::Restart),
            boxed("revision", "Revision invalidates prior quality facts", O::RevisionInvalidation),
            boxed("role-drift", "Role and work ownership drift is rejected", O::RoleDrift),
            boxed("stale-evidence", "Stale evidence cannot advance", O::StaleEvidence),
            boxed("teardown", "Teardown failure remains explicit", O::TeardownIsolation),
        ],
    )
}

fn boxed<S: OrchestratorConformanceSubject + 'static>(
    suffix: &'static str,
    summary: &'static str,
    scenario: OrchestratorScenario,
) -> Box<dyn ConformanceCase<S>> {
    Box::new(OrchestratorCase {
        descriptor: CaseDescriptor::new(
            CaseId::new(format!("peritus.orchestrator.{suffix}"))
                .expect("static orchestrator case identifier"),
            ReportText::literal(summary),
        ),
        scenario,
    })
}

fn result<S: OrchestratorConformanceSubject>(
    subject: &mut S,
    scenario: OrchestratorScenario,
) -> CaseResult {
    let fixture = OrchestratorConformanceFixture::new(scenario);
    let Ok(observed) = subject.exercise(&fixture) else {
        return failed(scenario, 0, 0, false);
    };
    let bounded = observed.revisions <= fixture.maximum_revisions()
        && observed.directives <= fixture.maximum_directives();
    let common = bounded && observed.limits_enforced && observed.no_implicit_acceptance;
    let accepted = observed.terminal == OrchestratorTerminal::Accepted;
    let exact = common
        && match scenario {
            OrchestratorScenario::HappyPath => {
                accepted
                    && observed.phase_order_exact
                    && observed.ownership_exact
                    && observed.b0_acceptance_observed
            }
            OrchestratorScenario::FixCycle => accepted && observed.fix_cycle_exact,
            OrchestratorScenario::RoleDrift => !accepted && observed.ownership_drift_rejected,
            OrchestratorScenario::StaleEvidence | OrchestratorScenario::RevisionInvalidation => {
                !accepted && observed.stale_evidence_rejected
            }
            OrchestratorScenario::LimitExhaustion => {
                observed.terminal == OrchestratorTerminal::Exhausted
            }
            OrchestratorScenario::PauseResume => observed.pause_reconciled,
            OrchestratorScenario::Cancellation => {
                observed.terminal == OrchestratorTerminal::Cancelled
                    && observed.cancellation_dominates
            }
            OrchestratorScenario::Restart => {
                observed.replay_equivalent && observed.idempotent_recovery
            }
            OrchestratorScenario::MalformedProtocol => !accepted && observed.malformed_rejected,
            OrchestratorScenario::PanicContainment => observed.panic_contained,
            OrchestratorScenario::TeardownIsolation => observed.teardown_explicit,
        };
    if exact {
        CaseResult::passed(observations(observed.revisions, observed.directives, true))
    } else {
        failed(scenario, observed.revisions, observed.directives, bounded)
    }
}

fn failed(
    scenario: OrchestratorScenario,
    revisions: u16,
    directives: u16,
    exact: bool,
) -> CaseResult {
    CaseResult::failed(observations(revisions, directives, exact), assertion(scenario))
}

fn observations(revisions: u16, directives: u16, exact: bool) -> Vec<Observation> {
    vec![
        Observation::new(
            ObservationId::catalog("revisions"),
            ObservationValue::Unsigned(u64::from(revisions)),
        ),
        Observation::new(
            ObservationId::catalog("directives"),
            ObservationValue::Unsigned(u64::from(directives)),
        ),
        Observation::new(ObservationId::catalog("exact"), ObservationValue::Boolean(exact)),
    ]
}

fn assertion(scenario: OrchestratorScenario) -> AssertionFailure {
    let number = match scenario {
        OrchestratorScenario::HappyPath => "001",
        OrchestratorScenario::FixCycle => "002",
        OrchestratorScenario::RoleDrift => "003",
        OrchestratorScenario::StaleEvidence => "004",
        OrchestratorScenario::RevisionInvalidation => "005",
        OrchestratorScenario::LimitExhaustion => "006",
        OrchestratorScenario::PauseResume => "007",
        OrchestratorScenario::Cancellation => "008",
        OrchestratorScenario::Restart => "009",
        OrchestratorScenario::MalformedProtocol => "010",
        OrchestratorScenario::PanicContainment => "011",
        OrchestratorScenario::TeardownIsolation => "012",
    };
    AssertionFailure::new(
        FailureCode::new(format!("PERITUS-ORCHESTRATOR-CONFORMANCE-{number}"))
            .expect("static orchestrator failure code"),
        ReportText::literal("E0 direct observations violated the selected orchestration contract"),
        None,
        None,
    )
}
