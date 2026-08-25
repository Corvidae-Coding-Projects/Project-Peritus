use peritus_conformance::{
    CaseDescriptor, CaseStatus, ConformanceFuture, ConformanceRunner, SubjectDescriptor,
    SubjectFactory, SubjectFailure, SuiteStatus, TraceConformanceError, TraceConformanceFixture,
    TraceConformanceObservation, TraceConformanceSubject, TraceScenario, trace_suite,
};

use super::harness::{block_on, text};

struct ReferenceTrace {
    leak_sensitive: bool,
}

impl TraceConformanceSubject for ReferenceTrace {
    fn exercise(
        &mut self,
        fixture: &TraceConformanceFixture,
    ) -> Result<TraceConformanceObservation, TraceConformanceError> {
        let dropped = u64::from(matches!(
            fixture.scenario(),
            TraceScenario::BoundedLoad | TraceScenario::Backpressure
        ));
        Ok(TraceConformanceObservation {
            accepted: 7,
            dropped,
            exported: 6,
            peak_buffered: fixture.queue_capacity(),
            causal_integrity: true,
            duplicate_integrity: true,
            replay_equivalent: true,
            leakage_free: !self.leak_sensitive,
            accounting_exact: true,
            failure_retained: true,
            recovery_exact: true,
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
            descriptor: SubjectDescriptor::new(text("trace-reference"), text("A2 C7 oracle")),
            leak_sensitive,
        }
    }
}

impl SubjectFactory<ReferenceTrace> for Factory {
    fn descriptor(&self) -> &SubjectDescriptor {
        &self.descriptor
    }

    fn create<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
    ) -> ConformanceFuture<'a, Result<ReferenceTrace, SubjectFailure>> {
        let leak_sensitive = self.leak_sensitive;
        Box::pin(async move { Ok(ReferenceTrace { leak_sensitive }) })
    }

    fn teardown<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
        _subject: ReferenceTrace,
    ) -> ConformanceFuture<'a, Result<(), SubjectFailure>> {
        Box::pin(async { Ok(()) })
    }
}

#[test]
fn trace_catalog_runs_all_nine_cases() {
    let factory = Factory::new(false);
    let report = block_on(ConformanceRunner::run(&trace_suite::<ReferenceTrace>(), &factory));
    assert_eq!(report.status(), SuiteStatus::Passed, "{report:?}");
    assert_eq!(report.summary().total(), 9);
}

#[test]
fn trace_catalog_rejects_default_surface_leakage() {
    let factory = Factory::new(true);
    let report = block_on(ConformanceRunner::run(&trace_suite::<ReferenceTrace>(), &factory));
    assert!(report.cases().iter().any(|case| {
        case.status() == CaseStatus::Failed
            && case.descriptor().id().as_str() == "peritus.trace.redaction-leakage"
    }));
}
