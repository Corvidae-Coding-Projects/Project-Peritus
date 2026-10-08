//! Shared cancellation observation for every product-run phase.

use std::sync::atomic::Ordering;

use super::ProductRunInput;
use crate::{ProductRunnerError, ProductRunnerErrorKind};

/// Rejects further active work after either run-owned cancellation signal is set.
pub fn check_cancelled(input: &ProductRunInput) -> Result<(), ProductRunnerError> {
    if input.cancelled.load(Ordering::Acquire) || input.provider_cancellation.is_cancelled() {
        Err(cancelled())
    } else {
        Ok(())
    }
}

/// Finalization has separate ownership after the active provider deadline has fired.
pub(super) fn check_finalization_cancelled(
    input: &ProductRunInput,
) -> Result<(), ProductRunnerError> {
    if input.cancelled.load(Ordering::Acquire) { Err(cancelled()) } else { Ok(()) }
}

fn cancelled() -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::Cancelled,
        "execute coding run",
        "run was cancelled",
    )
}
