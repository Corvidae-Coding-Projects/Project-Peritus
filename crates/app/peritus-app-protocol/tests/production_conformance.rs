//! Production A3 subject coverage for the full A2 application-protocol suite.

mod support;

#[path = "production_conformance/scenarios.rs"]
mod scenarios;

use peritus_conformance::{
    CaseDescriptor, ConformanceFuture, ConformanceRunner, ProtocolConformanceError,
    ProtocolConformanceFixture, ProtocolConformanceObservation, ProtocolConformanceSubject,
    SubjectDescriptor, SubjectFactory, SubjectFailure, SuiteStatus, protocol_suite,
};
use std::future::Future;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
use std::thread;

struct ProductionProtocol;

impl ProtocolConformanceSubject for ProductionProtocol {
    fn exercise(
        &mut self,
        fixture: &ProtocolConformanceFixture,
    ) -> Result<ProtocolConformanceObservation, ProtocolConformanceError> {
        Ok(scenarios::observe(fixture))
    }
}

struct Factory {
    descriptor: SubjectDescriptor,
}

impl Factory {
    fn new() -> Self {
        Self {
            descriptor: SubjectDescriptor::new(
                "peritus-app-protocol".try_into().expect("static subject id"),
                "production A3 contract".try_into().expect("static description"),
            ),
        }
    }
}

impl SubjectFactory<ProductionProtocol> for Factory {
    fn descriptor(&self) -> &SubjectDescriptor {
        &self.descriptor
    }

    fn create<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
    ) -> ConformanceFuture<'a, Result<ProductionProtocol, SubjectFailure>> {
        Box::pin(async { Ok(ProductionProtocol) })
    }

    fn teardown<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
        _subject: ProductionProtocol,
    ) -> ConformanceFuture<'a, Result<(), SubjectFailure>> {
        Box::pin(async { Ok(()) })
    }
}

#[test]
fn production_protocol_passes_all_sixteen_a2_cases() {
    let report =
        block_on(ConformanceRunner::run(&protocol_suite::<ProductionProtocol>(), &Factory::new()));
    assert_eq!(report.status(), SuiteStatus::Passed, "{report:?}");
    assert_eq!(report.summary().total(), 16);
    assert_eq!(report.summary().passed(), 16);
}

struct TestWake(thread::Thread);

impl Wake for TestWake {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.unpark();
    }
}

fn block_on<F: Future>(future: F) -> F::Output {
    let waker = Waker::from(Arc::new(TestWake(thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = Box::pin(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => thread::park(),
        }
    }
}
