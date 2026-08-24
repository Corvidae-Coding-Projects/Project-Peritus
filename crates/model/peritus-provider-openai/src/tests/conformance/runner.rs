//! Fresh-subject execution of all fourteen A2 provider cases.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use peritus_conformance::{
    CaseDescriptor, ConformanceFuture, ConformanceRunner, ReportText, SubjectDescriptor,
    SubjectFactory, SubjectFailure, SuiteStatus, provider_suite,
};

use super::super::support::block_on;
use super::Subject;

struct Factory {
    descriptor: SubjectDescriptor,
    created: Arc<AtomicUsize>,
}

impl SubjectFactory<Subject> for Factory {
    fn descriptor(&self) -> &SubjectDescriptor {
        &self.descriptor
    }

    fn create<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
    ) -> ConformanceFuture<'a, Result<Subject, SubjectFailure>> {
        self.created.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(Subject) })
    }

    fn teardown<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
        _subject: Subject,
    ) -> ConformanceFuture<'a, Result<(), SubjectFailure>> {
        Box::pin(async { Ok(()) })
    }
}

#[test]
fn production_adapter_passes_all_provider_cases_with_fresh_subjects() {
    let created = Arc::new(AtomicUsize::new(0));
    let factory = Factory {
        descriptor: SubjectDescriptor::new(
            ReportText::new("OpenAI Responses").expect("name"),
            ReportText::new("production adapter v1").expect("implementation"),
        ),
        created: Arc::clone(&created),
    };
    let suite = provider_suite::<Subject>();
    let report = block_on(ConformanceRunner::run(&suite, &factory));
    assert_eq!(report.status(), SuiteStatus::Passed, "{report:?}");
    assert_eq!(created.load(Ordering::SeqCst), 14);
}
