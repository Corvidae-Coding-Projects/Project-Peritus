use peritus_conformance::{
    CaseDescriptor, CaseStatus, ConformanceFuture, ConformanceRunner, OrchestratorConformanceError,
    OrchestratorConformanceFixture, OrchestratorConformanceObservation,
    OrchestratorConformanceSubject, OrchestratorScenario, OrchestratorTerminal, SubjectDescriptor,
    SubjectFactory, SubjectFailure, SuiteStatus, orchestrator_suite,
};

use super::harness::{block_on, text};

struct ReferenceOrchestrator {
    permit_stale_acceptance: bool,
}

impl OrchestratorConformanceSubject for ReferenceOrchestrator {
    fn exercise(
        &mut self,
        fixture: &OrchestratorConformanceFixture,
    ) -> Result<OrchestratorConformanceObservation, OrchestratorConformanceError> {
        let terminal = match fixture.scenario() {
            OrchestratorScenario::RoleDrift => OrchestratorTerminal::Rejected,
            OrchestratorScenario::StaleEvidence | OrchestratorScenario::RevisionInvalidation
                if self.permit_stale_acceptance =>
            {
                OrchestratorTerminal::Accepted
            }
            OrchestratorScenario::StaleEvidence | OrchestratorScenario::RevisionInvalidation => {
                OrchestratorTerminal::NeedsHuman
            }
            OrchestratorScenario::LimitExhaustion => OrchestratorTerminal::Exhausted,
            OrchestratorScenario::Cancellation => OrchestratorTerminal::Cancelled,
            OrchestratorScenario::MalformedProtocol => OrchestratorTerminal::Failed,
            _ => OrchestratorTerminal::Accepted,
        };
        Ok(OrchestratorConformanceObservation {
            terminal,
            revisions: 2,
            directives: 8,
            phase_order_exact: true,
            ownership_exact: true,
            ownership_drift_rejected: true,
            stale_evidence_rejected: !self.permit_stale_acceptance,
            fix_cycle_exact: true,
            limits_enforced: true,
            pause_reconciled: true,
            cancellation_dominates: true,
            replay_equivalent: true,
            idempotent_recovery: true,
            malformed_rejected: true,
            b0_acceptance_observed: true,
            panic_contained: true,
            teardown_explicit: true,
            no_implicit_acceptance: !self.permit_stale_acceptance,
        })
    }
}

struct Factory {
    descriptor: SubjectDescriptor,
    permit_stale_acceptance: bool,
}

impl Factory {
    fn new(permit_stale_acceptance: bool) -> Self {
        Self {
            descriptor: SubjectDescriptor::new(
                text("orchestrator-reference"),
                text("A2 E0 oracle"),
            ),
            permit_stale_acceptance,
        }
    }
}

impl SubjectFactory<ReferenceOrchestrator> for Factory {
    fn descriptor(&self) -> &SubjectDescriptor {
        &self.descriptor
    }

    fn create<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
    ) -> ConformanceFuture<'a, Result<ReferenceOrchestrator, SubjectFailure>> {
        let permit_stale_acceptance = self.permit_stale_acceptance;
        Box::pin(async move { Ok(ReferenceOrchestrator { permit_stale_acceptance }) })
    }

    fn teardown<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
        _subject: ReferenceOrchestrator,
    ) -> ConformanceFuture<'a, Result<(), SubjectFailure>> {
        Box::pin(async { Ok(()) })
    }
}

#[test]
fn orchestrator_catalog_runs_all_twelve_cases() {
    let report = block_on(ConformanceRunner::run(
        &orchestrator_suite::<ReferenceOrchestrator>(),
        &Factory::new(false),
    ));
    assert_eq!(report.status(), SuiteStatus::Passed, "{report:?}");
    assert_eq!(report.summary().total(), 12);
}

#[test]
fn orchestrator_catalog_rejects_stale_implicit_acceptance() {
    let report = block_on(ConformanceRunner::run(
        &orchestrator_suite::<ReferenceOrchestrator>(),
        &Factory::new(true),
    ));
    assert!(report.cases().iter().any(|case| {
        case.status() == CaseStatus::Failed
            && case.descriptor().id().as_str() == "peritus.orchestrator.stale-evidence"
    }));
}
