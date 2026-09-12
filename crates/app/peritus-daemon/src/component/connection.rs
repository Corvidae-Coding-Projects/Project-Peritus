//! User-triggered connection tests through the production registry and credential broker.

use super::{PlatformCredentialSource, ProviderRegistry, ProviderRegistryLimits};
use crate::{DaemonError, DaemonErrorCode, DaemonRecovery, ProviderRoute};
use peritus_provider_core::{
    CancellationToken,
    connection::{ConnectionReport, verify_provider_connection},
};
use std::sync::Arc;

/// Tests one explicitly selected route with the same adapters and OS credentials as a real run.
///
/// # Errors
/// Returns a safe construction or stage-specific connection failure. No workspace is opened.
pub async fn test_provider_connection(
    route: &ProviderRoute,
) -> Result<ConnectionReport, DaemonError> {
    let declaration = route.declaration()?;
    let id = declaration.profile().profile_id();
    let broker = PlatformCredentialSource::providers();
    let registry = ProviderRegistry::build(
        vec![declaration],
        ProviderRegistryLimits::PRODUCTION,
        Some(Arc::new(broker)),
    )
    .map_err(connection_error)?;
    let provider = registry
        .current_provider(id)
        .ok_or_else(|| connection_error("selected provider was not registered"))?;
    verify_provider_connection(provider.as_ref(), CancellationToken::new())
        .await
        .map_err(connection_error)
}

fn connection_error(error: impl core::fmt::Display) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::InvalidInput,
        DaemonRecovery::CorrectRequest,
        "test provider connection",
        error.to_string(),
    )
}
