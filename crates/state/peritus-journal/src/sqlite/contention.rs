//! Explicit ownership for cancellation-aware `SQLite` contention waits.

use std::{
    cell::RefCell,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use crate::JournalError;
use rusqlite::Connection;

std::thread_local! {
    static ACTIVE_CANCELLATIONS: RefCell<Vec<JournalCancellation>> = const {
        RefCell::new(Vec::new())
    };
}

/// Explicit cancellation for a journal owner waiting on transient `SQLite` contention.
///
/// The token is separate from command identity. Cancelling a wait never changes or replaces the
/// command being resolved, and a later retry must use the original command and request digest.
#[derive(Clone, Default)]
pub struct JournalCancellation {
    cancelled: Arc<AtomicBool>,
    external: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
}

impl std::fmt::Debug for JournalCancellation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("JournalCancellation")
            .field("cancelled", &self.is_cancelled()).finish_non_exhaustive()
    }
}

impl JournalCancellation {
    /// Creates a live cancellation token.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Observes an existing owner's cancellation signal without a watcher thread.
    /// The check runs on the waiting thread and must be nonblocking and nonpanicking.
    #[must_use]
    pub fn with_cancellation_check(check: impl Fn() -> bool + Send + Sync + 'static) -> Self {
        Self { cancelled: Arc::new(AtomicBool::new(false)), external: Some(Arc::new(check)) }
    }

    /// Requests cancellation of the current or queued contention wait.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    /// Returns whether cancellation has been requested.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
            || self.external.as_ref().is_some_and(|check| check())
    }

    /// Runs journal work with this token visible to the connection's contention handler.
    ///
    /// Nested scopes restore the previous token even when the operation unwinds.
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

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum ContentionPolicy {
    Timeout(Duration),
    WaitForCancellation,
}

pub(super) fn configure(
    connection: &Connection,
    contention: ContentionPolicy,
) -> Result<(), JournalError> {
    match contention {
        ContentionPolicy::Timeout(timeout) => connection
            .busy_timeout(timeout)
            .map_err(|error| JournalError::sqlite("configure busy timeout", error)),
        ContentionPolicy::WaitForCancellation => connection
            .busy_handler(Some(wait_for_contention))
            .map_err(|error| JournalError::sqlite("configure contention wait", error)),
    }
}

fn wait_for_contention(_prior_attempts: i32) -> bool {
    let cancelled = ACTIVE_CANCELLATIONS
        .with(|active| active.borrow().last().is_some_and(JournalCancellation::is_cancelled));
    if cancelled {
        return false;
    }
    // This is retry cadence, not a patience deadline. The handler continues until ownership is
    // released or the active caller explicitly cancels its wait.
    std::thread::sleep(Duration::from_millis(1));
    true
}
