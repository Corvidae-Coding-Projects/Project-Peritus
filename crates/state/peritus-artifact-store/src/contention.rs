//! Explicit ownership for cancellation-aware artifact-catalog contention waits.

use std::{
    cell::RefCell,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use rusqlite::Connection;

use crate::ArtifactStoreError;

std::thread_local! {
    static ACTIVE_CANCELLATIONS: RefCell<Vec<ArtifactCatalogCancellation>> = const {
        RefCell::new(Vec::new())
    };
}

/// Explicit cancellation for an owner waiting on transient artifact-catalog contention.
///
/// Cancelling a wait does not discard the writer, reservation, or publication receipt. Callers
/// may retry the same operation with the same [`crate::ArtifactWriteHandle`].
#[derive(Clone, Default)]
pub struct ArtifactCatalogCancellation {
    cancelled: Arc<AtomicBool>,
    external: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
}

impl std::fmt::Debug for ArtifactCatalogCancellation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("ArtifactCatalogCancellation")
            .field("cancelled", &self.is_cancelled()).finish_non_exhaustive()
    }
}

impl ArtifactCatalogCancellation {
    /// Creates a live cancellation token.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Observes the caller's existing cancellation signal without a watcher thread.
    /// The check runs on the waiting thread and must be nonblocking and nonpanicking.
    #[must_use]
    pub fn with_cancellation_check(check: impl Fn() -> bool + Send + Sync + 'static) -> Self {
        Self { cancelled: Arc::new(AtomicBool::new(false)), external: Some(Arc::new(check)) }
    }

    /// Requests cancellation of the active or queued catalog wait.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    /// Returns whether cancellation has been requested.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
            || self.external.as_ref().is_some_and(|check| check())
    }

    /// Runs catalog work under this owner's cancellation scope.
    /// Nested scopes restore the prior owner on return or unwind. Cancellation only
    /// stops contention waits; accepted writers and receipts retain their identity.
    pub fn run<T>(&self, operation: impl FnOnce() -> T) -> T {
        struct Restore;
        impl Drop for Restore {
            fn drop(&mut self) {
                ACTIVE_CANCELLATIONS.with(|active| {
                    active.borrow_mut().pop();
                });
            }
        }

        ACTIVE_CANCELLATIONS.with(|active| active.borrow_mut().push(self.clone()));
        let restore = Restore;
        let result = operation();
        drop(restore);
        result
    }
}

pub(super) fn configure(connection: &Connection) -> Result<(), ArtifactStoreError> {
    connection.busy_handler(Some(wait_for_contention)).map_err(crate::catalog::catalog_error)
}

pub(super) fn active_wait_cancelled() -> bool {
    ACTIVE_CANCELLATIONS
        .with(|active| active.borrow().last().is_some_and(ArtifactCatalogCancellation::is_cancelled))
}

fn wait_for_contention(_prior_attempts: i32) -> bool {
    if active_wait_cancelled() {
        return false;
    }
    // Retry cadence only. Ownership release or explicit cancellation, rather than elapsed time,
    // decides when this wait ends.
    std::thread::sleep(Duration::from_millis(1));
    true
}
