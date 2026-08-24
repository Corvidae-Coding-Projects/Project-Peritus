//! A2 process suite executed against the production process gateway and local launcher.

#[path = "support/subject.rs"]
mod subject;
#[path = "support/subject_authorization.rs"]
mod subject_authorization;
#[path = "support/subject_recovery.rs"]
mod subject_recovery;
mod support;

use std::{
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
    thread,
};

use peritus_conformance::{
    CaseDescriptor, ConformanceFuture, ConformanceRunner, ReportText, SubjectDescriptor,
    SubjectFactory, SubjectFailure, SuiteStatus, process_suite,
};

use subject::ProductionProcessSubject;
use support::authority::commit_authority_without_dispatch;
use support::{
    Ids, PlanOptions, TestRoot, commit_authority, commit_authority_with_lease, intent,
    open_journal, plan,
};

struct Factory {
    descriptor: SubjectDescriptor,
    created: Arc<AtomicUsize>,
    torn_down: Arc<AtomicUsize>,
}

impl Factory {
    fn new() -> Self {
        Self {
            descriptor: SubjectDescriptor::new(
                ReportText::new("peritus-process").expect("subject name"),
                ReportText::new("production gateway, process owner, PTY, and recovery adapter")
                    .expect("subject summary"),
            ),
            created: Arc::new(AtomicUsize::new(0)),
            torn_down: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl SubjectFactory<ProductionProcessSubject> for Factory {
    fn descriptor(&self) -> &SubjectDescriptor {
        &self.descriptor
    }

    fn create<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
    ) -> ConformanceFuture<'a, Result<ProductionProcessSubject, SubjectFailure>> {
        self.created.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(ProductionProcessSubject::new()) })
    }

    fn teardown<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
        _subject: ProductionProcessSubject,
    ) -> ConformanceFuture<'a, Result<(), SubjectFailure>> {
        self.torn_down.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(()) })
    }
}

#[test]
fn production_process_passes_complete_a2_suite_with_fresh_case_subjects() {
    let factory = Factory::new();
    let report =
        block_on(ConformanceRunner::run(&process_suite::<ProductionProcessSubject>(), &factory));
    assert_eq!(report.status(), SuiteStatus::Passed, "{report:#?}");
    assert_eq!(report.summary().total(), 10);
    assert_eq!(factory.created.load(Ordering::SeqCst), 10);
    assert_eq!(factory.torn_down.load(Ordering::SeqCst), 10);
}

fn block_on<T>(future: impl Future<Output = T>) -> T {
    struct ThreadWake(thread::Thread);
    impl Wake for ThreadWake {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
        fn wake_by_ref(self: &Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(ThreadWake(thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = Box::pin(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => thread::park(),
        }
    }
}
