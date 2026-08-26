use peritus_conformance::{
    CaseDescriptor, CaseStatus, ConformanceFuture, ConformanceRunner, EvaluationConformanceError,
    EvaluationConformanceFixture, EvaluationConformanceObservation, EvaluationConformanceSubject,
    EvaluationScenario, EvaluationTerminal, SubjectDescriptor, SubjectFactory, SubjectFailure,
    SuiteStatus, evaluation_suite,
};

use super::harness::{block_on, text};

struct ReferenceEvaluation {
    leak_sensitive: bool,
}

impl EvaluationConformanceSubject for ReferenceEvaluation {
    fn exercise(
        &mut self,
        fixture: &EvaluationConformanceFixture,
    ) -> Result<EvaluationConformanceObservation, EvaluationConformanceError> {
        let terminal = match fixture.scenario() {
            EvaluationScenario::MalformedInput => EvaluationTerminal::Rejected,
            EvaluationScenario::Cancellation => EvaluationTerminal::Cancelled,
            _ => EvaluationTerminal::Completed,
        };
        Ok(EvaluationConformanceObservation {
            terminal,
            planned_rollouts: 12,
            maximum_attempts: 2,
            report_metrics: 11,
            frozen_inputs_exact: true,
            isolation_exact: true,
            deterministic: true,
            accounting_complete: true,
            statistics_valid: true,
            infrastructure_distinct: true,
            cancellation_durable: true,
            replay_equivalent: true,
            malformed_rejected: true,
            publication_ordered: true,
            redaction_safe: !self.leak_sensitive,
            bounds_enforced: true,
            panic_contained: true,
            teardown_explicit: true,
            non_authoritative: true,
        })
    }
}

struct Factory {
    descriptor: SubjectDescriptor,
    leak_sensitive: bool,
}

impl Factory {
    fn new(leak_sensitive: bool) -> Self {
        Self {
            descriptor: SubjectDescriptor::new(text("evaluation-reference"), text("A2 E3 oracle")),
            leak_sensitive,
        }
    }
}

impl SubjectFactory<ReferenceEvaluation> for Factory {
    fn descriptor(&self) -> &SubjectDescriptor {
        &self.descriptor
    }

    fn create<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
    ) -> ConformanceFuture<'a, Result<ReferenceEvaluation, SubjectFailure>> {
        let leak_sensitive = self.leak_sensitive;
        Box::pin(async move { Ok(ReferenceEvaluation { leak_sensitive }) })
    }

    fn teardown<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
        _subject: ReferenceEvaluation,
    ) -> ConformanceFuture<'a, Result<(), SubjectFailure>> {
        Box::pin(async { Ok(()) })
    }
}

#[test]
fn evaluation_catalog_runs_all_thirteen_cases() {
    let report = block_on(ConformanceRunner::run(
        &evaluation_suite::<ReferenceEvaluation>(),
        &Factory::new(false),
    ));
    assert_eq!(report.status(), SuiteStatus::Passed, "{report:?}");
    assert_eq!(report.summary().total(), 13);
}

#[test]
fn evaluation_catalog_rejects_default_surface_leakage() {
    let report = block_on(ConformanceRunner::run(
        &evaluation_suite::<ReferenceEvaluation>(),
        &Factory::new(true),
    ));
    assert!(report.cases().iter().any(|case| {
        case.status() == CaseStatus::Failed
            && case.descriptor().id().as_str() == "peritus.evaluation.redaction"
    }));
}
