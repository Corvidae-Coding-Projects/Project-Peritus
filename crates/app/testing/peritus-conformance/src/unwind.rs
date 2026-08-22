//! In-process unwind capture for callbacks and boxed asynchronous operations.

use std::any::Any;
use std::future::Future;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::{ConformanceFuture, FailurePhase, PanicFailure, PanicMessage};

pub fn callback<T>(phase: FailurePhase, action: impl FnOnce() -> T) -> Result<T, PanicFailure> {
    catch_unwind(AssertUnwindSafe(action)).map_err(|payload| panic_failure(phase, payload))
}

pub struct GuardedFuture<'a, T> {
    future: Option<ConformanceFuture<'a, T>>,
    phase: FailurePhase,
}

impl<'a, T> GuardedFuture<'a, T> {
    pub(crate) const fn new(future: ConformanceFuture<'a, T>, phase: FailurePhase) -> Self {
        Self { future: Some(future), phase }
    }
}

impl<T> Future for GuardedFuture<'_, T> {
    type Output = Result<T, PanicFailure>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let Some(mut future) = this.future.take() else {
            return Poll::Ready(Err(PanicFailure::new(
                this.phase,
                vec![PanicMessage::normalized("completed conformance future was polled again")],
            )));
        };
        match catch_unwind(AssertUnwindSafe(|| future.as_mut().poll(context))) {
            Ok(Poll::Pending) => {
                this.future = Some(future);
                Poll::Pending
            }
            Ok(Poll::Ready(output)) => match catch_unwind(AssertUnwindSafe(|| drop(future))) {
                Ok(()) => Poll::Ready(Ok(output)),
                Err(payload) => {
                    let mut failure = panic_failure(this.phase, payload);
                    if let Err(output_payload) = catch_unwind(AssertUnwindSafe(|| drop(output))) {
                        append_payload(&mut failure, output_payload);
                    }
                    Poll::Ready(Err(failure))
                }
            },
            Err(payload) => {
                let mut failure = panic_failure(this.phase, payload);
                if let Err(drop_payload) = catch_unwind(AssertUnwindSafe(|| drop(future))) {
                    append_payload(&mut failure, drop_payload);
                }
                Poll::Ready(Err(failure))
            }
        }
    }
}

fn panic_failure(phase: FailurePhase, payload: Box<dyn Any + Send>) -> PanicFailure {
    PanicFailure::new(phase, consume_payload(payload))
}

fn append_payload(failure: &mut PanicFailure, payload: Box<dyn Any + Send>) {
    for message in consume_payload(payload) {
        failure.push_message(message);
    }
}

fn consume_payload(payload: Box<dyn Any + Send>) -> Vec<PanicMessage> {
    let mut messages = vec![normalize_payload(&*payload)];
    if let Err(drop_payload) = catch_unwind(AssertUnwindSafe(|| drop(payload))) {
        messages.push(normalize_payload(&*drop_payload));
        std::mem::forget(drop_payload);
    }
    messages
}

fn normalize_payload(payload: &(dyn Any + Send)) -> PanicMessage {
    payload.downcast_ref::<String>().map_or_else(
        || {
            payload.downcast_ref::<&'static str>().map_or_else(
                || PanicMessage::normalized("non-string panic payload"),
                |message| PanicMessage::normalized(message),
            )
        },
        |message| PanicMessage::normalized(message),
    )
}
