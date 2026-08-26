use peritus_conformance::{
    CaseDescriptor, ConformanceFuture, ConformanceRunner, EvolutionConformanceError,
    EvolutionConformanceFixture, EvolutionConformanceObservation, EvolutionConformanceSubject,
    EvolutionScenario, EvolutionTerminal, SubjectDescriptor, SubjectFactory, SubjectFailure,
    SuiteStatus, evolution_suite,
};

use super::harness::{block_on, text};

struct ReferenceEvolution;

impl EvolutionConformanceSubject for ReferenceEvolution {
    fn exercise(
        &mut self,
        fixture: &EvolutionConformanceFixture,
    ) -> Result<EvolutionConformanceObservation, EvolutionConformanceError> {
        let terminal = match fixture.scenario() {
            EvolutionScenario::RollbackHistory => EvolutionTerminal::RolledBack,
            EvolutionScenario::Contamination
            | EvolutionScenario::MetricGaming
            | EvolutionScenario::StaleEvidence
            | EvolutionScenario::MalformedInput
            | EvolutionScenario::Bounds => EvolutionTerminal::Rejected,
            _ => EvolutionTerminal::Promoted,
        };
        Ok(EvolutionConformanceObservation {
            terminal,
            manifests: 3,
            variants: 3,
            criteria: 12,
            activation_history: 2,
            frozen_evidence_exact: true,
            change_isolation_exact: true,
            interaction_attribution_exact: true,
            contamination_rejected: true,
            metric_gaming_rejected: true,
            selection_deterministic: true,
            stale_evidence_rejected: true,
            review_exact: true,
            authority_exact: true,
            activation_atomic: true,
            rollback_auditable: true,
            replay_equivalent: true,
            malformed_rejected: true,
            bounds_enforced: true,
            evidence_exact: true,
            non_self_promoting: true,
        })
    }
}

struct Factory {
    descriptor: SubjectDescriptor,
}

impl Factory {
    fn new() -> Self {
        Self {
            descriptor: SubjectDescriptor::new(text("evolution-reference"), text("A2 F0 oracle")),
        }
    }
}

impl SubjectFactory<ReferenceEvolution> for Factory {
    fn descriptor(&self) -> &SubjectDescriptor {
        &self.descriptor
    }

    fn create<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
    ) -> ConformanceFuture<'a, Result<ReferenceEvolution, SubjectFailure>> {
        Box::pin(async { Ok(ReferenceEvolution) })
    }

    fn teardown<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
        _subject: ReferenceEvolution,
    ) -> ConformanceFuture<'a, Result<(), SubjectFailure>> {
        Box::pin(async { Ok(()) })
    }
}

#[test]
fn evolution_catalog_runs_all_fourteen_cases() {
    let report =
        block_on(ConformanceRunner::run(&evolution_suite::<ReferenceEvolution>(), &Factory::new()));
    assert_eq!(report.status(), SuiteStatus::Passed, "{report:?}");
    assert_eq!(report.summary().total(), 14);
}
