//! Cooperative cancellation for an active MCP bridge request.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use tokio::sync::Notify;

/// Cloneable idempotent cancellation signal passed into a daemon bridge operation.
#[derive(Clone, Debug, Default)]
pub struct McpCancellation {
    state: Arc<CancellationState>,
}

#[derive(Debug, Default)]
struct CancellationState {
    cancelled: AtomicBool,
    notify: Notify,
}

impl McpCancellation {
    /// Creates an active signal.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Requests cancellation, returning `true` only for the state-changing call.
    #[must_use]
    pub fn cancel(&self) -> bool {
        if self.state.cancelled.swap(true, Ordering::SeqCst) {
            false
        } else {
            self.state.notify.notify_waiters();
            true
        }
    }

    /// Reports whether cancellation was requested.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.state.cancelled.load(Ordering::SeqCst)
    }

    /// Waits until cancellation is requested.
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
