use peritus_conformance::{
    CaseDescriptor, CaseStatus, CollaborationConformanceError, CollaborationConformanceFixture,
    CollaborationConformanceObservation, CollaborationConformanceSubject, CollaborationScenario,
    CollaborationTerminal, ConformanceFuture, ConformanceRunner, SubjectDescriptor, SubjectFactory,
    SubjectFailure, SuiteStatus, collaboration_suite,
};

use super::harness::{block_on, text};

struct ReferenceCollaboration {
    violate_parentage: bool,
}

impl CollaborationConformanceSubject for ReferenceCollaboration {
    fn exercise(
        &mut self,
        fixture: &CollaborationConformanceFixture,
    ) -> Result<CollaborationConformanceObservation, CollaborationConformanceError> {
        let terminal = match fixture.scenario() {
            CollaborationScenario::BoundedGraph => CollaborationTerminal::Exhausted,
            CollaborationScenario::CancellationTree => CollaborationTerminal::Cancelled,
            _ => CollaborationTerminal::Completed,
        };
        Ok(CollaborationConformanceObservation {
            terminal,
            tasks: 8,
            peak_depth: 3,
            peak_fanout: 3,
            messages: 12,
            parentage_valid: !self.violate_parentage,
            delegation_exact: true,
            bounds_enforced: true,
            messages_causal: true,
            all_join_truthful: true,
            any_join_truthful: true,
            handoff_exact: true,
            cancellation_complete: true,
            replay_equivalent: true,
            idempotent_recovery: true,
            no_implicit_success: true,
        })
    }
}

struct Factory {
    descriptor: SubjectDescriptor,
    violate_parentage: bool,
}

impl Factory {
    fn new(violate_parentage: bool) -> Self {
        Self {
            descriptor: SubjectDescriptor::new(
                text("collaboration-reference"),
                text("A2 D3 causal oracle"),
            ),
            violate_parentage,
        }
    }
}

impl SubjectFactory<ReferenceCollaboration> for Factory {
    fn descriptor(&self) -> &SubjectDescriptor {
        &self.descriptor
    }

    fn create<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
    ) -> ConformanceFuture<'a, Result<ReferenceCollaboration, SubjectFailure>> {
        let violate_parentage = self.violate_parentage;
        Box::pin(async move { Ok(ReferenceCollaboration { violate_parentage }) })
    }

    fn teardown<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
        _subject: ReferenceCollaboration,
    ) -> ConformanceFuture<'a, Result<(), SubjectFailure>> {
        Box::pin(async { Ok(()) })
    }
}

#[test]
fn collaboration_catalog_runs_all_ten_cases() {
    let report = block_on(ConformanceRunner::run(
        &collaboration_suite::<ReferenceCollaboration>(),
        &Factory::new(false),
    ));
    assert_eq!(report.status(), SuiteStatus::Passed, "{report:?}");
    assert_eq!(report.summary().total(), 10);
}

#[test]
fn collaboration_catalog_rejects_invalid_parentage() {
    let report = block_on(ConformanceRunner::run(
        &collaboration_suite::<ReferenceCollaboration>(),
        &Factory::new(true),
    ));
    assert!(report.cases().iter().any(|case| {
        case.status() == CaseStatus::Failed
            && case.descriptor().id().as_str() == "peritus.collaboration.causal-parentage"
    }));
}
