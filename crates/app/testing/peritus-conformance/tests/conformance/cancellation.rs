use std::future::Future;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Wake, Waker};

use peritus_conformance::{
    CaseDescriptor, CaseId, CaseResult, ConformanceCase, ConformanceFuture, ConformanceRunner,
    StaticSuite, SubjectDescriptor, SubjectFactory, SubjectFailure, SuiteDescriptor, SuiteId,
};

use super::harness::text;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct CancellationState {
    subjects_dropped: usize,
    teardowns_called: usize,
    teardown_polls: usize,
    teardown_completions: usize,
    teardown_futures_dropped: usize,
}

struct CancellationSubject {
    state: Arc<Mutex<CancellationState>>,
}

impl Drop for CancellationSubject {
    fn drop(&mut self) {
        self.state.lock().expect("cancellation state lock").subjects_dropped += 1;
    }
}

struct CancellationFactory {
    descriptor: SubjectDescriptor,
    state: Arc<Mutex<CancellationState>>,
    pending_teardown: bool,
    teardown_drop_panics: bool,
}

impl CancellationFactory {
    fn new() -> Self {
        Self {
            descriptor: SubjectDescriptor::new(text("cancellation-subject"), text("raii-test")),
            state: Arc::new(Mutex::new(CancellationState::default())),
            pending_teardown: false,
            teardown_drop_panics: false,
        }
    }

    fn with_pending_teardown(teardown_drop_panics: bool) -> Self {
        Self { pending_teardown: true, teardown_drop_panics, ..Self::new() }
    }

    fn state(&self) -> CancellationState {
        *self.state.lock().expect("cancellation state lock")
    }
}

impl SubjectFactory<CancellationSubject> for CancellationFactory {
    fn descriptor(&self) -> &SubjectDescriptor {
        &self.descriptor
    }

    fn create<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
    ) -> ConformanceFuture<'a, Result<CancellationSubject, SubjectFailure>> {
        let state = Arc::clone(&self.state);
        Box::pin(async move { Ok(CancellationSubject { state }) })
    }

    fn teardown<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
        subject: CancellationSubject,
    ) -> ConformanceFuture<'a, Result<(), SubjectFailure>> {
        self.state.lock().expect("cancellation state lock").teardowns_called += 1;
        let state = Arc::clone(&self.state);
        if self.pending_teardown {
            Box::pin(PendingTeardownFuture {
                subject: Some(subject),
                state,
                panic_on_drop: self.teardown_drop_panics,
            })
        } else {
            Box::pin(async move {
                state.lock().expect("cancellation state lock").teardown_completions += 1;
                drop(subject);
                Ok(())
            })
        }
    }
}

struct PendingCase {
    descriptor: CaseDescriptor,
    panic_on_drop: bool,
}

impl PendingCase {
    fn new(panic_on_drop: bool) -> Self {
        Self {
            descriptor: CaseDescriptor::new(
                CaseId::new("case.pending-cancellation").expect("valid case ID"),
                text("pending cancellation case"),
            ),
            panic_on_drop,
        }
    }
}

impl ConformanceCase<CancellationSubject> for PendingCase {
    fn descriptor(&self) -> &CaseDescriptor {
        &self.descriptor
    }

    fn run<'a>(
        &'a self,
        _subject: &'a mut CancellationSubject,
    ) -> ConformanceFuture<'a, CaseResult> {
        Box::pin(PendingCaseFuture { panic_on_drop: self.panic_on_drop })
    }
}

struct PendingCaseFuture {
    panic_on_drop: bool,
}

impl Future for PendingCaseFuture {
    type Output = CaseResult;

    fn poll(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Pending
    }
}

impl Drop for PendingCaseFuture {
    fn drop(&mut self) {
        assert!(!self.panic_on_drop, "pending case future drop panic");
    }
}

struct PassingCase {
    descriptor: CaseDescriptor,
}

impl PassingCase {
    fn new() -> Self {
        Self {
            descriptor: CaseDescriptor::new(
                CaseId::new("case.pending-teardown").expect("valid case ID"),
                text("pending teardown case"),
            ),
        }
    }
}

impl ConformanceCase<CancellationSubject> for PassingCase {
    fn descriptor(&self) -> &CaseDescriptor {
        &self.descriptor
    }

    fn run<'a>(
        &'a self,
        _subject: &'a mut CancellationSubject,
    ) -> ConformanceFuture<'a, CaseResult> {
        Box::pin(async { CaseResult::passed(Vec::new()) })
    }
}

struct PendingTeardownFuture {
    subject: Option<CancellationSubject>,
    state: Arc<Mutex<CancellationState>>,
    panic_on_drop: bool,
}

impl Future for PendingTeardownFuture {
    type Output = Result<(), SubjectFailure>;

    fn poll(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Self::Output> {
        self.state.lock().expect("cancellation state lock").teardown_polls += 1;
        Poll::Pending
    }
}

impl Drop for PendingTeardownFuture {
    fn drop(&mut self) {
        self.state.lock().expect("cancellation state lock").teardown_futures_dropped += 1;
        let subject = self.subject.take();
        drop(subject);
        assert!(!self.panic_on_drop, "pending teardown future drop panic");
    }
}

struct NoopWake;

impl Wake for NoopWake {
    fn wake(self: Arc<Self>) {
        drop(self);
    }
}

fn suite(panic_on_drop: bool) -> StaticSuite<CancellationSubject> {
    StaticSuite::new(
        SuiteDescriptor::new(
            SuiteId::new("test.cancellation").expect("valid suite ID"),
            text("runner cancellation behavior"),
        ),
        vec![Box::new(PendingCase::new(panic_on_drop))],
    )
}

fn teardown_suite() -> StaticSuite<CancellationSubject> {
    StaticSuite::new(
        SuiteDescriptor::new(
            SuiteId::new("test.teardown-cancellation").expect("valid suite ID"),
            text("teardown cancellation behavior"),
        ),
        vec![Box::new(PassingCase::new())],
    )
}

fn poll_once_then_drop(panic_on_drop: bool) -> (CancellationState, bool) {
    let suite = suite(panic_on_drop);
    let factory = CancellationFactory::new();
    let mut run = Box::pin(ConformanceRunner::run(&suite, &factory));
    let waker = Waker::from(Arc::new(NoopWake));
    let mut context = Context::from_waker(&waker);
    assert!(run.as_mut().poll(&mut context).is_pending());

    let drop_panicked = catch_unwind(AssertUnwindSafe(|| drop(run))).is_err();
    (factory.state(), drop_panicked)
}

fn poll_pending_teardown_then_drop(panic_on_drop: bool) -> (CancellationState, bool) {
    let suite = teardown_suite();
    let factory = CancellationFactory::with_pending_teardown(panic_on_drop);
    let mut run = Box::pin(ConformanceRunner::run(&suite, &factory));
    let waker = Waker::from(Arc::new(NoopWake));
    let mut context = Context::from_waker(&waker);
    assert!(run.as_mut().poll(&mut context).is_pending());

    let drop_panicked = catch_unwind(AssertUnwindSafe(|| drop(run))).is_err();
    (factory.state(), drop_panicked)
}

#[test]
fn cancelling_pending_run_uses_subject_raii_without_async_teardown() {
    let (state, drop_panicked) = poll_once_then_drop(false);
    assert!(!drop_panicked);
    assert_eq!(state.subjects_dropped, 1);
    assert_eq!(state.teardowns_called, 0);
}

#[test]
fn pending_future_drop_panic_unwinds_outside_report_containment() {
    let (state, drop_panicked) = poll_once_then_drop(true);
    assert!(drop_panicked);
    assert_eq!(state.subjects_dropped, 1);
    assert_eq!(state.teardowns_called, 0);
}

#[test]
fn cancelling_pending_teardown_drops_owned_subject_without_completing_future() {
    let (state, drop_panicked) = poll_pending_teardown_then_drop(false);
    assert!(!drop_panicked);
    assert_eq!(state.teardowns_called, 1);
    assert_eq!(state.teardown_polls, 1);
    assert_eq!(state.teardown_completions, 0);
    assert_eq!(state.teardown_futures_dropped, 1);
    assert_eq!(state.subjects_dropped, 1);
}

#[test]
fn pending_teardown_drop_panic_also_unwinds_outside_report_containment() {
    let (state, drop_panicked) = poll_pending_teardown_then_drop(true);
    assert!(drop_panicked);
    assert_eq!(state.teardowns_called, 1);
    assert_eq!(state.teardown_completions, 0);
    assert_eq!(state.teardown_futures_dropped, 1);
    assert_eq!(state.subjects_dropped, 1);
}
