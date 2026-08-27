//! Cooperative first-request-wins cancellation delivered to one worker task.

use tokio::sync::watch;

use super::WorkerCancellationReason;

/// Cloneable read-only cancellation token supplied to a supervised task.
#[derive(Clone)]
pub(crate) struct WorkerCancellation {
    receiver: watch::Receiver<Option<WorkerCancellationReason>>,
}

impl WorkerCancellation {
    pub(super) const fn new(receiver: watch::Receiver<Option<WorkerCancellationReason>>) -> Self {
        Self { receiver }
    }

    /// Returns the first requested cancellation reason, when present.
    #[must_use]
    pub(crate) fn reason(&self) -> Option<WorkerCancellationReason> {
        *self.receiver.borrow()
    }

    /// Returns whether cancellation has been requested.
    #[must_use]
    pub(crate) fn is_cancelled(&self) -> bool {
        self.reason().is_some()
    }

    /// Waits until cancellation is requested.
    ///
    /// A closed sender is conservatively classified as daemon shutdown.
    pub(crate) async fn cancelled(&mut self) -> WorkerCancellationReason {
        loop {
            if let Some(reason) = self.reason() {
                return reason;
            }
            if self.receiver.changed().await.is_err() {
                return WorkerCancellationReason::Shutdown;
            }
        }
    }
}
