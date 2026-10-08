//! Explicit cancellation ownership for projection `SQLite` contention waits.

use std::{cell::RefCell, time::Duration};

use peritus_journal::JournalCancellation;
use rusqlite::Connection;

use crate::ProjectionError;

std::thread_local! {
    static ACTIVE_CANCELLATIONS: RefCell<Vec<JournalCancellation>> = const {
        RefCell::new(Vec::new())
    };
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum ContentionPolicy {
    FailFast,
    Timeout(Duration),
    WaitForCancellation,
}

pub(super) fn configure(
    connection: &Connection,
    contention: ContentionPolicy,
) -> Result<(), ProjectionError> {
    match contention {
        ContentionPolicy::FailFast => connection
            .busy_timeout(Duration::ZERO)
            .map_err(|error| ProjectionError::sqlite("configure fail-fast contention", error)),
        ContentionPolicy::Timeout(timeout) => connection
            .busy_timeout(timeout)
            .map_err(|error| ProjectionError::sqlite("configure projection busy timeout", error)),
        ContentionPolicy::WaitForCancellation => connection
            .busy_handler(Some(wait_for_contention))
            .map_err(|error| ProjectionError::sqlite("configure projection contention wait", error)),
    }
}

pub(super) fn run<T>(
    cancellation: Option<&JournalCancellation>,
    operation: impl FnOnce() -> T,
) -> T {
    let Some(cancellation) = cancellation else {
        return operation();
    };

    struct Restore;
    impl Drop for Restore {
        fn drop(&mut self) {
            ACTIVE_CANCELLATIONS.with(|active| {
                active.borrow_mut().pop();
            });
        }
    }

    ACTIVE_CANCELLATIONS.with(|active| active.borrow_mut().push(cancellation.clone()));
    let restore = Restore;
    let result = operation();
    drop(restore);
    result
}

fn wait_for_contention(_prior_attempts: i32) -> bool {
    let retry = ACTIVE_CANCELLATIONS.with(|active| {
        active.borrow().last().is_some_and(|cancellation| !cancellation.is_cancelled())
    });
    if retry {
        std::thread::sleep(Duration::from_millis(1));
    }
    retry
}
