use peritus_conformance::{
    CaseDescriptor, CaseStatus, ConformanceFuture, ConformanceRunner, GateConformanceError,
    GateConformanceFixture, GateConformanceObservation, GateConformanceSubject, GateScenario,
    GateTerminal, SubjectDescriptor, SubjectFactory, SubjectFailure, SuiteStatus, gate_suite,
};

use super::harness::{block_on, text};

struct ReferenceGate {
    allow_malformed_pass: bool,
}

impl GateConformanceSubject for ReferenceGate {
    fn exercise(
        &mut self,
        fixture: &GateConformanceFixture,
    ) -> Result<GateConformanceObservation, GateConformanceError> {
        let terminal = match fixture.scenario() {
            GateScenario::InspectEditRunTest | GateScenario::ArtifactEvidence => {
                GateTerminal::Passed
            }
            GateScenario::FailedPrerequisite => GateTerminal::Blocked,
            GateScenario::Cancellation => GateTerminal::Cancelled,
            GateScenario::MalformedParser | GateScenario::StaleRevision
                if self.allow_malformed_pass =>
            {
                GateTerminal::Passed
            }
            _ => GateTerminal::Failed,
        };
        Ok(GateConformanceObservation {
            terminal,
            peak_attempt: 2,
            dispatches: 4,
            dependencies_ordered: true,
            revision_exact: true,
            clean_snapshot: true,
            no_implicit_success: !self.allow_malformed_pass,
            authority_before_effect: true,
            replay_equivalent: true,
            idempotent_recovery: true,
            evidence_complete: true,
            stable_aggregation: true,
        })
    }
}

struct Factory {
    descriptor: SubjectDescriptor,
    allow_malformed_pass: bool,
}

impl Factory {
    fn new(allow_malformed_pass: bool) -> Self {
        Self {
            descriptor: SubjectDescriptor::new(text("gate-reference"), text("A2 D1 oracle")),
            allow_malformed_pass,
        }
    }
}

impl SubjectFactory<ReferenceGate> for Factory {
    fn descriptor(&self) -> &SubjectDescriptor {
        &self.descriptor
    }

    fn create<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
    ) -> ConformanceFuture<'a, Result<ReferenceGate, SubjectFailure>> {
        let allow_malformed_pass = self.allow_malformed_pass;
        Box::pin(async move { Ok(ReferenceGate { allow_malformed_pass }) })
    }

    fn teardown<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
        _subject: ReferenceGate,
    ) -> ConformanceFuture<'a, Result<(), SubjectFailure>> {
        Box::pin(async { Ok(()) })
    }
}

#[test]
fn gate_catalog_runs_all_ten_cases() {
    let factory = Factory::new(false);
    let report = block_on(ConformanceRunner::run(&gate_suite::<ReferenceGate>(), &factory));
    assert_eq!(report.status(), SuiteStatus::Passed, "{report:?}");
    assert_eq!(report.summary().total(), 10);
}

#[test]
fn gate_catalog_rejects_implicit_success() {
    let factory = Factory::new(true);
    let report = block_on(ConformanceRunner::run(&gate_suite::<ReferenceGate>(), &factory));
    assert!(report.cases().iter().any(|case| {
        case.status() == CaseStatus::Failed
            && case.descriptor().id().as_str() == "peritus.gate.malformed-parser"
    }));
}
