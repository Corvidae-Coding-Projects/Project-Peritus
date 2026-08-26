use peritus_conformance::{
    CaseDescriptor, CaseStatus, ConformanceFuture, ConformanceRunner, DebuggerConformanceError,
    DebuggerConformanceFixture, DebuggerConformanceObservation, DebuggerConformanceSubject,
    DebuggerScenario, DebuggerTerminal, SubjectDescriptor, SubjectFactory, SubjectFailure,
    SuiteStatus, debugger_suite,
};

use super::harness::{block_on, text};

struct ReferenceDebugger {
    leak_sensitive: bool,
}

impl DebuggerConformanceSubject for ReferenceDebugger {
    fn exercise(
        &mut self,
        fixture: &DebuggerConformanceFixture,
    ) -> Result<DebuggerConformanceObservation, DebuggerConformanceError> {
        let terminal = match fixture.scenario() {
            DebuggerScenario::ModelOutputRejection
            | DebuggerScenario::MalformedInput
            | DebuggerScenario::BoundedResources => DebuggerTerminal::Rejected,
            DebuggerScenario::Cancellation => DebuggerTerminal::Cancelled,
            _ => DebuggerTerminal::Completed,
        };
        Ok(DebuggerConformanceObservation {
            terminal,
            selected_events: 12,
            timeline_entries: 18,
            causes: 4,
            patterns: 3,
            selection_exact: true,
            timeline_exact: true,
            taxonomy_complete: true,
            citations_contained: true,
            model_rejection_exact: true,
            clustering_deterministic: true,
            replay_equivalent: true,
            cancellation_durable: true,
            malformed_rejected: true,
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
            descriptor: SubjectDescriptor::new(text("debugger-reference"), text("A2 E2 oracle")),
            leak_sensitive,
        }
    }
}

impl SubjectFactory<ReferenceDebugger> for Factory {
    fn descriptor(&self) -> &SubjectDescriptor {
        &self.descriptor
    }

    fn create<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
    ) -> ConformanceFuture<'a, Result<ReferenceDebugger, SubjectFailure>> {
        let leak_sensitive = self.leak_sensitive;
        Box::pin(async move { Ok(ReferenceDebugger { leak_sensitive }) })
    }

    fn teardown<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
        _subject: ReferenceDebugger,
    ) -> ConformanceFuture<'a, Result<(), SubjectFailure>> {
        Box::pin(async { Ok(()) })
    }
}

#[test]
fn debugger_catalog_runs_all_thirteen_cases() {
    let report = block_on(ConformanceRunner::run(
        &debugger_suite::<ReferenceDebugger>(),
        &Factory::new(false),
    ));
    assert_eq!(report.status(), SuiteStatus::Passed, "{report:?}");
    assert_eq!(report.summary().total(), 13);
}

#[test]
fn debugger_catalog_rejects_default_surface_leakage() {
    let report = block_on(ConformanceRunner::run(
        &debugger_suite::<ReferenceDebugger>(),
        &Factory::new(true),
    ));
    assert!(report.cases().iter().any(|case| {
        case.status() == CaseStatus::Failed
            && case.descriptor().id().as_str() == "peritus.debugger.redaction"
    }));
}
