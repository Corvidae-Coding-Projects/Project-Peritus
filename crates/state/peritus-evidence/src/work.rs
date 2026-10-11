//! Cooperative ownership and cancellation for incremental evidence work.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use crate::{EvidenceError, EvidenceErrorKind, RecoveryAction};

/// Cloneable cancellation signal checked between bounded I/O operations.
///
/// Cancellation never discards the operation's retained progress. A fresh token may be used to
/// continue that same operation. Callers supplying a blocking stream remain responsible for its
/// transport timeout or interruption; a token cannot interrupt an arbitrary `Read` implementation.
#[derive(Clone, Debug, Default)]
pub struct EvidenceCancellation(Arc<AtomicBool>);

impl EvidenceCancellation {
    /// Creates a live cancellation signal.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Requests cancellation without discarding owned work.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    /// Reports whether cancellation was requested.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    pub(crate) fn check(&self, operation: &'static str) -> Result<(), EvidenceError> {
        if self.is_cancelled() { Err(EvidenceError::cancelled(operation)) } else { Ok(()) }
    }
}

impl EvidenceError {
    pub(crate) fn cancelled(operation: &'static str) -> Self {
        Self::new(
            EvidenceErrorKind::Cancelled,
            RecoveryAction::Retry,
            operation,
            "evidence work cancelled with its owned progress retained",
        )
    }
}
