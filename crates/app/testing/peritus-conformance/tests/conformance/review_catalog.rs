use peritus_conformance::{
    CaseDescriptor, CaseStatus, ConformanceFuture, ConformanceRunner, ReviewConformanceError,
    ReviewConformanceFixture, ReviewConformanceObservation, ReviewConformanceSubject,
    ReviewScenario, ReviewTerminal, SubjectDescriptor, SubjectFactory, SubjectFailure, SuiteStatus,
    review_suite,
};

use super::harness::{block_on, text};

struct ReferenceReview {
    allow_stale_completion: bool,
}

impl ReviewConformanceSubject for ReferenceReview {
    fn exercise(
        &mut self,
        fixture: &ReviewConformanceFixture,
    ) -> Result<ReviewConformanceObservation, ReviewConformanceError> {
        let terminal = match fixture.scenario() {
            ReviewScenario::StaleRevision if self.allow_stale_completion => {
                ReviewTerminal::Completed
            }
            ReviewScenario::StaleRevision | ReviewScenario::Oscillation => {
                ReviewTerminal::NeedsHuman
            }
            ReviewScenario::MalformedSubmission => ReviewTerminal::Failed,
            _ => ReviewTerminal::Completed,
        };
        Ok(ReviewConformanceObservation {
            terminal,
            cycles: 2,
            findings: 2,
            revision_exact: true,
            quorum_complete: true,
            independence_complete: true,
            provenance_retained: true,
            findings_conserved: true,
            reviewer_confirmed: true,
            waiver_external: true,
            stale_rejected: !self.allow_stale_completion,
            replay_equivalent: true,
            idempotent_recovery: true,
            oscillation_truthful: true,
            malformed_rejected: true,
            no_implicit_success: !self.allow_stale_completion,
        })
    }
}

struct Factory {
    descriptor: SubjectDescriptor,
    allow_stale_completion: bool,
}

impl Factory {
    fn new(allow_stale_completion: bool) -> Self {
        Self {
            descriptor: SubjectDescriptor::new(text("review-reference"), text("A2 D2 oracle")),
            allow_stale_completion,
        }
    }
}

impl SubjectFactory<ReferenceReview> for Factory {
    fn descriptor(&self) -> &SubjectDescriptor {
        &self.descriptor
    }

    fn create<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
    ) -> ConformanceFuture<'a, Result<ReferenceReview, SubjectFailure>> {
        let allow_stale_completion = self.allow_stale_completion;
        Box::pin(async move { Ok(ReferenceReview { allow_stale_completion }) })
    }

    fn teardown<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
        _subject: ReferenceReview,
    ) -> ConformanceFuture<'a, Result<(), SubjectFailure>> {
        Box::pin(async { Ok(()) })
    }
}

#[test]
fn review_catalog_runs_all_ten_cases() {
    let report =
        block_on(ConformanceRunner::run(&review_suite::<ReferenceReview>(), &Factory::new(false)));
    assert_eq!(report.status(), SuiteStatus::Passed, "{report:?}");
    assert_eq!(report.summary().total(), 10);
}

#[test]
fn review_catalog_rejects_stale_implicit_completion() {
    let report =
        block_on(ConformanceRunner::run(&review_suite::<ReferenceReview>(), &Factory::new(true)));
    assert!(report.cases().iter().any(|case| {
        case.status() == CaseStatus::Failed
            && case.descriptor().id().as_str() == "peritus.review.stale-revision"
    }));
}
