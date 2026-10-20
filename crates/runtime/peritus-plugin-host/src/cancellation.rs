//! Race-safe cooperative host cancellation.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use tokio::sync::Notify;

/// Cloneable, idempotent cancellation token for plugin invocations.
#[derive(Clone, Debug, Default)]
pub struct HostCancellation {
    state: Arc<CancellationState>,
}

#[derive(Debug, Default)]
struct CancellationState {
    cancelled: AtomicBool,
    notify: Notify,
}

impl HostCancellation {
    /// Creates an active token.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Requests cancellation and wakes current waiters.
    ///
    /// Returns `true` only when this call changes the state.
    #[must_use]
    pub fn cancel(&self) -> bool {
        if self.state.cancelled.swap(true, Ordering::SeqCst) {
            false
        } else {
            self.state.notify.notify_waiters();
            true
        }
    }

    /// Returns whether cancellation was requested.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.state.cancelled.load(Ordering::SeqCst)
    }

    /// Waits for cancellation without a missed-notification race.
    pub async fn cancelled(&self) {
        loop {
            let notified = self.state.notify.notified();
            if self.is_cancelled() {
                return;
            }
            notified.await;
        }
    }
}
