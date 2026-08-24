//! A2 tool suite executed through fresh production protocol/router adapters.

mod conformance_support;
mod support;

use std::{
    future::Future,
    sync::Arc,
    task::{Context, Poll, Wake, Waker},
    thread,
};

use conformance_support::ProductionToolSubject;
use peritus_conformance::{
    CaseDescriptor, ConformanceFuture, ConformanceRunner, FailureCode, ReportText,
    SubjectDescriptor, SubjectFactory, SubjectFailure, SuiteStatus, tool_suite,
};

struct Factory(SubjectDescriptor);

impl Factory {
    fn new() -> Self {
        Self(SubjectDescriptor::new(
            ReportText::new("peritus-tool-router").expect("fixed subject name"),
            ReportText::new("production C4 protocol and router adapter")
                .expect("fixed subject summary"),
        ))
    }
}

impl SubjectFactory<ProductionToolSubject> for Factory {
    fn descriptor(&self) -> &SubjectDescriptor {
        &self.0
    }

    fn create<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
    ) -> ConformanceFuture<'a, Result<ProductionToolSubject, SubjectFailure>> {
        Box::pin(async { Ok(ProductionToolSubject::new(171)) })
    }

    fn teardown<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
        _subject: ProductionToolSubject,
    ) -> ConformanceFuture<'a, Result<(), SubjectFailure>> {
        Box::pin(async { Ok(()) })
    }
}

#[test]
fn production_router_passes_all_eight_a2_tool_cases() {
    let report =
        block_on(ConformanceRunner::run(&tool_suite::<ProductionToolSubject>(), &Factory::new()));
    assert_eq!(report.status(), SuiteStatus::Passed, "{report:?}");
    assert_eq!(report.summary().total(), 8);
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

#[allow(dead_code)]
fn _factory_failure_vocabulary() -> SubjectFailure {
    SubjectFailure::new(
        FailureCode::new("C4-TOOL-SETUP").expect("fixed failure code"),
        ReportText::new("production tool setup failed").expect("fixed setup failure"),
    )
}
