//! Race-free cooperative cancellation without a runtime type in the public API.

use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};

/// A cloneable, idempotent cancellation signal.
#[derive(Clone, Debug, Default)]
pub struct CancellationToken {
    state: Arc<CancellationState>,
}

#[derive(Debug, Default)]
struct CancellationState {
    cancelled: AtomicBool,
    waiters: Mutex<Vec<Weak<Waiter>>>,
}

#[derive(Debug, Default)]
struct Waiter {
    waker: Mutex<Option<Waker>>,
    registered: AtomicBool,
}

impl CancellationToken {
    /// Creates a token in the active state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Signals cancellation and wakes every current waiter.
    ///
    /// Returns `true` only for the call that changed the state.
    #[must_use]
    pub fn cancel(&self) -> bool {
        if self.state.cancelled.swap(true, Ordering::SeqCst) {
            return false;
        }
        let mut locked_waiters = take_lock(&self.state.waiters);
        let waiters = locked_waiters.drain(..).collect::<Vec<_>>();
        drop(locked_waiters);
        for waiter in waiters.into_iter().filter_map(|waiter| waiter.upgrade()) {
            let waker = take_lock(&waiter.waker).take();
            if let Some(waker) = waker {
                waker.wake();
            }
        }
        true
    }

    /// Returns whether cancellation was signalled.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.state.cancelled.load(Ordering::SeqCst)
    }

    /// Returns a future that completes once cancellation is signalled.
    pub fn cancelled(&self) -> CancellationFuture<'_> {
        CancellationFuture { token: self, waiter: Arc::new(Waiter::default()) }
    }
}

/// Future returned by [`CancellationToken::cancelled`].
#[derive(Debug)]
#[must_use = "futures do nothing unless polled or awaited"]
pub struct CancellationFuture<'a> {
    token: &'a CancellationToken,
    waiter: Arc<Waiter>,
}

/// Cancellation-first selection between one token and one owned operation future.
///
/// This concrete combinator keeps polling order visible to the ordinary API checker and avoids
/// macro-generated control flow. `None` means cancellation won; `Some` carries the operation's
/// output. The operation future is dropped immediately when cancellation wins.
pub struct CancelFirst<'a, F> {
    cancellation: CancellationFuture<'a>,
    operation: Pin<Box<F>>,
}

/// Creates an auditable cancellation-first selection future.
pub fn first<F>(token: &CancellationToken, operation: F) -> CancelFirst<'_, F>
where
    F: Future,
{
    CancelFirst { cancellation: token.cancelled(), operation: Box::pin(operation) }
}

impl Future for CancellationFuture<'_> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        if self.token.is_cancelled() {
            return Poll::Ready(());
        }
        *take_lock(&self.waiter.waker) = Some(context.waker().clone());
        if !self.waiter.registered.swap(true, Ordering::SeqCst) {
            let mut waiters = take_lock(&self.token.state.waiters);
            waiters.retain(|waiter| waiter.strong_count() > 0);
            waiters.push(Arc::downgrade(&self.waiter));
        }
        if self.token.is_cancelled() { Poll::Ready(()) } else { Poll::Pending }
    }
}

impl<F> Future for CancelFirst<'_, F>
where
    F: Future,
{
    type Output = Option<F::Output>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        if Pin::new(&mut this.cancellation).poll(context).is_ready() {
            return Poll::Ready(None);
        }
        match this.operation.as_mut().poll(context) {
            Poll::Ready(output) => Poll::Ready(Some(output)),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl Drop for CancellationFuture<'_> {
    fn drop(&mut self) {
        if self.waiter.registered.load(Ordering::SeqCst) {
            let target = Arc::downgrade(&self.waiter);
            take_lock(&self.token.state.waiters).retain(|waiter| !Weak::ptr_eq(waiter, &target));
        }
    }
}

fn take_lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}
