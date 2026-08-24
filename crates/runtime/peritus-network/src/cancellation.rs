//! Cloneable process-local cancellation token.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// Idempotent cancellation shared by proxy owner and workers.
#[derive(Clone, Debug, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    /// Creates an open token.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
    /// Requests cancellation and returns whether this was the first request.
    #[must_use]
    pub fn cancel(&self) -> bool {
        !self.0.swap(true, Ordering::AcqRel)
    }
    /// Returns whether cancellation was requested.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}
