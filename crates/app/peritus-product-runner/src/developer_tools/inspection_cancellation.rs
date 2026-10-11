//! Run-owned cancellation checks used while streaming filesystem inspection.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use peritus_agent::DeveloperLoopError;
use peritus_provider_core::CancellationToken;

use super::path::tool;

#[derive(Clone, Default)]
pub(super) struct InspectionCancellation {
    cancelled: Option<Arc<AtomicBool>>,
    provider: Option<CancellationToken>,
}

impl InspectionCancellation {
    pub(super) const fn new(cancelled: Arc<AtomicBool>, provider: CancellationToken) -> Self {
        Self { cancelled: Some(cancelled), provider: Some(provider) }
    }

    pub(super) fn check(&self) -> Result<(), DeveloperLoopError> {
        let run_cancelled =
            self.cancelled.as_ref().is_some_and(|cancelled| cancelled.load(Ordering::Acquire));
        let provider_cancelled =
            self.provider.as_ref().is_some_and(CancellationToken::is_cancelled);
        if run_cancelled || provider_cancelled {
            Err(tool("workspace inspection was cancelled"))
        } else {
            Ok(())
        }
    }
}
