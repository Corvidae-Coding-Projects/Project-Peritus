//! Explicit cancellation shared by one owned Git operation and its controller.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// Cloneable cancellation signal; it cannot grant repository or mutation authority.
#[derive(Clone, Debug, Default)]
pub struct GitCancellation(Arc<AtomicBool>);

impl GitCancellation {
    /// Creates a fresh, uncancelled operation lifetime.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Requests cancellation of this operation and its currently owned subprocess tree.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    /// Reports whether the owner or controller requested cancellation.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}
