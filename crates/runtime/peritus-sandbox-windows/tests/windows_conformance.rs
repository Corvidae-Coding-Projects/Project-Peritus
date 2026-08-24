//! Complete fresh-subject A2 suite through the deterministic Windows native compiler.

mod windows_conformance_support;

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
    SubjectFactory, SubjectFailure, SuiteStatus, sandbox_suite,
};
use windows_conformance_support::WindowsConformanceSubject;

struct Factory {
    descriptor: SubjectDescriptor,
    created: Arc<AtomicUsize>,
    torn_down: Arc<AtomicUsize>,
}

impl Factory {
    fn new() -> Self {
        Self {
            descriptor: SubjectDescriptor::new(
                ReportText::new("peritus-sandbox-windows").expect("fixed subject name"),
                ReportText::new("Windows deterministic projection and native lifecycle")
                    .expect("fixed subject summary"),
            ),
            created: Arc::new(AtomicUsize::new(0)),
            torn_down: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl SubjectFactory<WindowsConformanceSubject> for Factory {
    fn descriptor(&self) -> &SubjectDescriptor {
        &self.descriptor
    }

    fn create<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
    ) -> ConformanceFuture<'a, Result<WindowsConformanceSubject, SubjectFailure>> {
        self.created.fetch_add(1, Ordering::Relaxed);
        Box::pin(async { Ok(WindowsConformanceSubject::new()) })
    }

    fn teardown<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
        _subject: WindowsConformanceSubject,
    ) -> ConformanceFuture<'a, Result<(), SubjectFailure>> {
        self.torn_down.fetch_add(1, Ordering::Relaxed);
        Box::pin(async { Ok(()) })
    }
}

#[test]
fn windows_adapter_passes_the_complete_a2_sandbox_suite() {
    let factory = Factory::new();
    let report =
        block_on(ConformanceRunner::run(&sandbox_suite::<WindowsConformanceSubject>(), &factory));
    assert_eq!(report.status(), SuiteStatus::Passed, "{report:?}");
    assert_eq!(report.summary().total(), 10);
    assert_eq!(factory.created.load(Ordering::Relaxed), 10);
    assert_eq!(factory.torn_down.load(Ordering::Relaxed), 10);
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
