//! In-process panic containment for callbacks and boxed futures.

use std::future::Future;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::{FailurePhase, PanicFailure, QualificationFuture};

pub fn callback<T>(phase: FailurePhase, action: impl FnOnce() -> T) -> Result<T, PanicFailure> {
    catch_unwind(AssertUnwindSafe(action)).map_err(|payload| {
        dispose_payload(payload);
        PanicFailure::new(phase)
    })
}

pub struct GuardedFuture<'a, T> {
    future: Option<QualificationFuture<'a, T>>,
    phase: FailurePhase,
}

impl<'a, T> GuardedFuture<'a, T> {
    pub const fn new(future: QualificationFuture<'a, T>, phase: FailurePhase) -> Self {
        Self { future: Some(future), phase }
    }
}

impl<T> Future for GuardedFuture<'_, T> {
    type Output = Result<T, PanicFailure>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let Some(mut future) = this.future.take() else {
            return Poll::Ready(Err(PanicFailure::new(this.phase)));
        };
        match catch_unwind(AssertUnwindSafe(|| future.as_mut().poll(context))) {
            Ok(Poll::Pending) => {
                this.future = Some(future);
                Poll::Pending
            }
            Ok(Poll::Ready(output)) => match catch_unwind(AssertUnwindSafe(|| drop(future))) {
                Ok(()) => Poll::Ready(Ok(output)),
                Err(payload) => {
                    dispose_payload(payload);
                    drop_output(output);
                    Poll::Ready(Err(PanicFailure::new(this.phase)))
                }
            },
            Err(payload) => {
                dispose_payload(payload);
                if let Err(drop_payload) = catch_unwind(AssertUnwindSafe(|| drop(future))) {
                    dispose_payload(drop_payload);
                }
                Poll::Ready(Err(PanicFailure::new(this.phase)))
            }
        }
    }
}

fn drop_output<T>(output: T) {
    if let Err(payload) = catch_unwind(AssertUnwindSafe(|| drop(output))) {
        dispose_payload(payload);
    }
}

fn dispose_payload(payload: Box<dyn std::any::Any + Send>) {
    if let Err(nested) = catch_unwind(AssertUnwindSafe(|| drop(payload))) {
        std::mem::forget(nested);
    }
}
