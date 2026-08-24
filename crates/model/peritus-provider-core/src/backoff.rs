//! Cancellation-aware execution of checked backoff delays.

use crate::{BoxFuture, CancellationToken, ProviderCoreError};

/// Waits for the delay in a checked retry plan or returns promptly on cancellation.
#[must_use = "the backoff future must be awaited"]
pub fn wait_for_backoff(
    plan: crate::RetryPlan,
    cancellation: &CancellationToken,
) -> BoxFuture<'_, Result<(), ProviderCoreError>> {
    Box::pin(async move {
        if plan.action() == crate::RetryAction::Stop || plan.delay().is_zero() {
            if cancellation.is_cancelled() {
                return Err(ProviderCoreError::cancelled("backoff"));
            }
            return Ok(());
        }
        match crate::cancellation::first(cancellation, tokio::time::sleep(plan.delay())).await {
            None => Err(ProviderCoreError::cancelled("backoff")),
            Some(()) => Ok(()),
        }
    })
}
