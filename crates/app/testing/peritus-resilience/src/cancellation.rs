//! Runner-owned cooperative cancellation.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Cloneable cancellation observation passed to each fresh subject.
#[derive(Clone, Debug)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    /// Returns whether the runner was cancelled or abandoned.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

pub struct CancellationOwner {
    token: CancellationToken,
    armed: bool,
}

impl CancellationOwner {
    pub fn new() -> Self {
        Self {
            token: CancellationToken { cancelled: Arc::new(AtomicBool::new(false)) },
            armed: true,
        }
    }

    pub fn token(&self) -> CancellationToken {
        self.token.clone()
    }

    fn cancel(&self) {
        self.token.cancelled.store(true, Ordering::Release);
    }

    pub const fn completed(&mut self) {
        self.armed = false;
    }
}

impl Drop for CancellationOwner {
    fn drop(&mut self) {
        if self.armed {
            self.cancel();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CancellationOwner;

    #[test]
    fn abandoning_owner_cancels_every_clone() {
        let owner = CancellationOwner::new();
        let token = owner.token();
        assert!(!token.is_cancelled());
        drop(owner);
        assert!(token.is_cancelled());
    }

    #[test]
    fn completed_owner_does_not_report_cancellation() {
        let mut owner = CancellationOwner::new();
        let token = owner.token();
        owner.completed();
        drop(owner);
        assert!(!token.is_cancelled());
    }
}
