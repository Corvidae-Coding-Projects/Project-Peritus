//! Fresh-subject execution of all fourteen provider-neutral A2 cases.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use peritus_conformance::{
    CaseDescriptor, ConformanceFuture, ConformanceRunner, ReportText, SubjectDescriptor,
    SubjectFactory, SubjectFailure, SuiteStatus, provider_suite,
};

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
fn codex_runtime_passes_all_cases_through_real_owned_processes() {
    let created = Arc::new(AtomicUsize::new(0));
    let factory = Factory {
        descriptor: SubjectDescriptor::new(
            ReportText::new("OpenAI Codex account runtime").expect("name"),
            ReportText::new("production Tokio process adapter with test-only executable")
                .expect("implementation"),
        ),
        created: Arc::clone(&created),
    };
    let suite = provider_suite::<Subject>();
    let runtime =
        tokio::runtime::Builder::new_current_thread().enable_all().build().expect("test runtime");
    let report = runtime.block_on(ConformanceRunner::run(&suite, &factory));
    assert_eq!(report.status(), SuiteStatus::Passed, "{report:?}");
    assert_eq!(created.load(Ordering::SeqCst), 14);
}
