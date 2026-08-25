use peritus_conformance::{
    CaseDescriptor, CaseStatus, ConformanceFuture, ConformanceRunner, SchedulerConformanceError,
    SchedulerConformanceFixture, SchedulerConformanceObservation, SchedulerConformanceSubject,
    SchedulerScenario, SchedulerTerminal, SubjectDescriptor, SubjectFactory, SubjectFailure,
    SuiteStatus, scheduler_suite,
};

use super::harness::{block_on, text};

struct ReferenceScheduler {
    violate_ownership: bool,
}

impl SchedulerConformanceSubject for ReferenceScheduler {
    fn exercise(
        &mut self,
        fixture: &SchedulerConformanceFixture,
    ) -> Result<SchedulerConformanceObservation, SchedulerConformanceError> {
        let terminal = match fixture.scenario() {
            SchedulerScenario::WorkerLoss => SchedulerTerminal::Ambiguous,
            SchedulerScenario::BoundedBackpressure => SchedulerTerminal::Exhausted,
            SchedulerScenario::CancellationTree => SchedulerTerminal::Cancelled,
            _ => SchedulerTerminal::Completed,
        };
        Ok(SchedulerConformanceObservation {
            terminal,
            work: 8,
            peak_attempt: 2,
            peak_bypass: 3,
            selection_deterministic: true,
            resources_conserved: true,
            dependencies_satisfied: true,
            ownership_unique: !self.violate_ownership,
            loss_truthful: true,
            backpressure_bounded: true,
            pause_respected: true,
            cancellation_complete: true,
            replay_equivalent: true,
            idempotent_recovery: true,
            no_implicit_success: true,
        })
    }
}

struct Factory {
    descriptor: SubjectDescriptor,
    violate_ownership: bool,
}

impl Factory {
    fn new(violate_ownership: bool) -> Self {
        Self {
            descriptor: SubjectDescriptor::new(text("scheduler-reference"), text("A2 D3 oracle")),
            violate_ownership,
        }
    }
}

impl SubjectFactory<ReferenceScheduler> for Factory {
    fn descriptor(&self) -> &SubjectDescriptor {
        &self.descriptor
    }

    fn create<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
    ) -> ConformanceFuture<'a, Result<ReferenceScheduler, SubjectFailure>> {
        let violate_ownership = self.violate_ownership;
        Box::pin(async move { Ok(ReferenceScheduler { violate_ownership }) })
    }

    fn teardown<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
        _subject: ReferenceScheduler,
    ) -> ConformanceFuture<'a, Result<(), SubjectFailure>> {
        Box::pin(async { Ok(()) })
    }
}

#[test]
fn scheduler_catalog_runs_all_ten_cases() {
    let report = block_on(ConformanceRunner::run(
        &scheduler_suite::<ReferenceScheduler>(),
        &Factory::new(false),
    ));
    assert_eq!(report.status(), SuiteStatus::Passed, "{report:?}");
    assert_eq!(report.summary().total(), 10);
}

#[test]
fn scheduler_catalog_rejects_nonunique_dispatch_ownership() {
    let report = block_on(ConformanceRunner::run(
        &scheduler_suite::<ReferenceScheduler>(),
        &Factory::new(true),
    ));
    assert!(report.cases().iter().any(|case| {
        case.status() == CaseStatus::Failed
            && case.descriptor().id().as_str() == "peritus.scheduler.worker-ownership"
    }));
}
