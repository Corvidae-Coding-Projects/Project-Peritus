use std::future::Future;
use std::panic::panic_any;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Wake, Waker};
use std::thread;

use peritus_conformance::{
    AssertionFailure, CaseDescriptor, CaseId, CaseResult, ConformanceCase, ConformanceFuture,
    FailureCode, Observation, ReportText, SubjectDescriptor, SubjectFactory, SubjectFailure,
};

pub(super) fn block_on<T>(future: impl Future<Output = T>) -> T {
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

pub(super) fn code(value: &str) -> FailureCode {
    FailureCode::new(value).expect("test failure code must be valid")
}

pub(super) fn text(value: impl Into<String>) -> ReportText {
    ReportText::new(value).expect("test report text must be valid")
}

pub(super) fn assertion(summary: &str) -> AssertionFailure {
    AssertionFailure::new(code("TEST-ASSERTION"), text(summary), None, None)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct TestSubject {
    pub(super) serial: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum OperationBehavior {
    Pass,
    TypedFailure,
    PanicConstruction,
    PanicPoll,
    PanicNonString,
    PanicOnFutureDrop,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct FactoryState {
    pub(super) created_for: Vec<String>,
    pub(super) torn_down: Vec<(String, usize)>,
}

pub(super) struct TestFactory {
    descriptor: SubjectDescriptor,
    descriptor_panics: bool,
    setup: OperationBehavior,
    teardown: OperationBehavior,
    state: Arc<Mutex<FactoryState>>,
}

impl TestFactory {
    pub(super) fn new(setup: OperationBehavior, teardown: OperationBehavior) -> Self {
        Self {
            descriptor: SubjectDescriptor::new(text("test-subject"), text("test-implementation")),
            descriptor_panics: false,
            setup,
            teardown,
            state: Arc::new(Mutex::new(FactoryState::default())),
        }
    }

    pub(super) fn descriptor_panics() -> Self {
        Self {
            descriptor_panics: true,
            ..Self::new(OperationBehavior::Pass, OperationBehavior::Pass)
        }
    }

    pub(super) fn state(&self) -> FactoryState {
        self.state.lock().expect("test state lock must be usable").clone()
    }
}

impl SubjectFactory<TestSubject> for TestFactory {
    fn descriptor(&self) -> &SubjectDescriptor {
        assert!(!self.descriptor_panics, "subject descriptor panic");
        &self.descriptor
    }

    fn create<'a>(
        &'a self,
        case: &'a CaseDescriptor,
    ) -> ConformanceFuture<'a, Result<TestSubject, SubjectFailure>> {
        let serial = {
            let mut state = self.state.lock().expect("test state lock must be usable");
            state.created_for.push(case.id().as_str().to_owned());
            state.created_for.len()
        };
        match self.setup {
            OperationBehavior::Pass => Box::pin(async move { Ok(TestSubject { serial }) }),
            OperationBehavior::TypedFailure => Box::pin(async {
                Err(SubjectFailure::new(code("TEST-SETUP"), text("setup failed")))
            }),
            OperationBehavior::PanicConstruction => panic!("setup construction panic"),
            OperationBehavior::PanicPoll => Box::pin(async { panic!("setup poll panic") }),
            OperationBehavior::PanicNonString => Box::pin(async { panic_any(7_u8) }),
            OperationBehavior::PanicOnFutureDrop => {
                Box::pin(DropPanicFuture::new(Ok(TestSubject { serial })))
            }
        }
    }

    fn teardown<'a>(
        &'a self,
        case: &'a CaseDescriptor,
        subject: TestSubject,
    ) -> ConformanceFuture<'a, Result<(), SubjectFailure>> {
        self.state
            .lock()
            .expect("test state lock must be usable")
            .torn_down
            .push((case.id().as_str().to_owned(), subject.serial));
        match self.teardown {
            OperationBehavior::Pass => Box::pin(async { Ok(()) }),
            OperationBehavior::TypedFailure => Box::pin(async {
                Err(SubjectFailure::new(code("TEST-TEARDOWN"), text("teardown failed")))
            }),
            OperationBehavior::PanicConstruction => panic!("teardown construction panic"),
            OperationBehavior::PanicPoll => Box::pin(async { panic!("teardown poll panic") }),
            OperationBehavior::PanicNonString => Box::pin(async { panic_any(9_u8) }),
            OperationBehavior::PanicOnFutureDrop => Box::pin(DropPanicFuture::new(Ok(()))),
        }
    }
}

#[derive(Clone, Debug)]
pub(super) enum CaseBehavior {
    Pass(Vec<Observation>),
    Fail,
    PanicConstruction,
    PanicPoll,
    PanicNonString,
    PanicOnFutureDrop,
    PanicPollAndDrop,
    PanicPayloadDrop,
    PanicLong,
    PendingOnce,
}

pub(super) struct TestCase {
    descriptor: CaseDescriptor,
    behavior: CaseBehavior,
    descriptor_panics: bool,
}

impl TestCase {
    pub(super) fn new(id: &str, behavior: CaseBehavior) -> Self {
        Self {
            descriptor: CaseDescriptor::new(
                CaseId::new(id).expect("test case ID must be valid"),
                text(format!("test case {id}")),
            ),
            behavior,
            descriptor_panics: false,
        }
    }

    pub(super) fn descriptor_panics(id: &str) -> Self {
        Self { descriptor_panics: true, ..Self::new(id, CaseBehavior::Pass(Vec::new())) }
    }
}

impl ConformanceCase<TestSubject> for TestCase {
    fn descriptor(&self) -> &CaseDescriptor {
        assert!(!self.descriptor_panics, "case descriptor panic");
        &self.descriptor
    }

    fn run<'a>(&'a self, subject: &'a mut TestSubject) -> ConformanceFuture<'a, CaseResult> {
        assert!(subject.serial > 0, "subject serial must be nonzero");
        match &self.behavior {
            CaseBehavior::Pass(observations) => {
                let observations = observations.clone();
                Box::pin(async move { CaseResult::passed(observations) })
            }
            CaseBehavior::Fail => {
                Box::pin(async { CaseResult::failed(Vec::new(), assertion("case failed")) })
            }
            CaseBehavior::PanicConstruction => panic!("case construction panic"),
            CaseBehavior::PanicPoll => Box::pin(async { panic!("case poll panic") }),
            CaseBehavior::PanicNonString => Box::pin(async { panic_any(11_u8) }),
            CaseBehavior::PanicOnFutureDrop => {
                Box::pin(DropPanicFuture::new(CaseResult::passed(Vec::new())))
            }
            CaseBehavior::PanicPollAndDrop => Box::pin(PollAndDropPanicFuture),
            CaseBehavior::PanicPayloadDrop => Box::pin(async { panic_any(PayloadDropPanic) }),
            CaseBehavior::PanicLong => {
                let message = "x".repeat(8_192);
                Box::pin(async move { panic!("{message}") })
            }
            CaseBehavior::PendingOnce => Box::pin(PendingOnce::new(CaseResult::passed(Vec::new()))),
        }
    }
}

struct PendingOnce<T> {
    output: Option<T>,
    pending: bool,
}

impl<T> PendingOnce<T> {
    const fn new(output: T) -> Self {
        Self { output: Some(output), pending: true }
    }
}

impl<T> Unpin for PendingOnce<T> {}

impl<T> Future for PendingOnce<T> {
    type Output = T;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<T> {
        if self.pending {
            self.pending = false;
            context.waker().wake_by_ref();
            Poll::Pending
        } else {
            Poll::Ready(self.output.take().expect("pending future has one output"))
        }
    }
}

struct DropPanicFuture<T> {
    output: Option<T>,
}

impl<T> DropPanicFuture<T> {
    const fn new(output: T) -> Self {
        Self { output: Some(output) }
    }
}

impl<T> Unpin for DropPanicFuture<T> {}

impl<T> Future for DropPanicFuture<T> {
    type Output = T;

    fn poll(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<T> {
        Poll::Ready(self.output.take().expect("drop-panic future has one output"))
    }
}

impl<T> Drop for DropPanicFuture<T> {
    fn drop(&mut self) {
        panic!("future drop panic");
    }
}

struct PollAndDropPanicFuture;

impl Future for PollAndDropPanicFuture {
    type Output = CaseResult;

    fn poll(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Self::Output> {
        panic!("future poll panic");
    }
}

impl Drop for PollAndDropPanicFuture {
    fn drop(&mut self) {
        panic!("future drop after poll panic");
    }
}

struct PayloadDropPanic;

impl Drop for PayloadDropPanic {
    fn drop(&mut self) {
        panic!("panic payload drop");
    }
}
