//! Immutable startup inventory for configured providers and tools.

use std::sync::Arc;

use peritus_tool_router::RouterLimits;

use super::{
    PlatformCredentialSource, ProviderRegistry, ProviderRegistryLimits, ToolComponentError,
    ToolComponents,
};
use crate::{DaemonConfig, DaemonError, DaemonErrorCode, DaemonRecovery};

/// Exact immutable component inventory retained for the daemon lifetime.
#[derive(Debug)]
pub struct DaemonComponents {
    providers: ProviderRegistry,
    tools: ToolComponents,
}

impl DaemonComponents {
    /// Constructs every configured revision-bound component exactly once.
    ///
    /// # Errors
    ///
    /// Returns a typed startup failure when a provider, credential backend, or tool inventory
    /// cannot be represented exactly.
    pub fn build(config: &DaemonConfig) -> Result<Self, DaemonError> {
        let direct =
            config.providers().iter().any(crate::ProviderRoute::requires_credential_broker);
        let credential_source = PlatformCredentialSource::new("peritus").map_err(provider_error)?;
        if direct && !credential_source.available() {
            return Err(DaemonError::new(
                DaemonErrorCode::RecoveryRequired,
                DaemonRecovery::ReadOnly,
                "probe platform credential store",
                "a configured direct provider requires an unavailable platform credential store",
            ));
        }
        let declarations = config
            .providers()
            .iter()
            .map(crate::ProviderRoute::declaration)
            .collect::<Result<Vec<_>, _>>()?;
        let providers = ProviderRegistry::build(
            declarations,
            ProviderRegistryLimits::PRODUCTION,
            Some(Arc::new(credential_source)),
        )
        .map_err(|error| {
            DaemonError::with_source(
                DaemonErrorCode::InvalidInput,
                DaemonRecovery::CorrectRequest,
                "construct provider registry",
                error.to_string(),
                error,
            )
        })?;
        let tool_limits =
            RouterLimits::new(config.limits().maximum_workers(), config.limits().authority_queue())
                .map_err(|error| {
                    DaemonError::with_source(
                        DaemonErrorCode::InvalidInput,
                        DaemonRecovery::CorrectRequest,
                        "configure tool router limits",
                        error.to_string(),
                        error,
                    )
                })?;
        let tools =
            ToolComponents::build(config.tools().allowed(), tool_limits).map_err(tool_error)?;
        Ok(Self { providers, tools })
    }

    /// Borrows the exact provider-profile registry.
    #[must_use]
    pub const fn providers(&self) -> &ProviderRegistry {
        &self.providers
    }

    /// Reports whether one exact canonical tool name is configured.
    #[must_use]
    pub fn tool_enabled(&self, name: &str) -> bool {
        self.tools.contains_name(name)
    }

    /// Returns configured tool names in deterministic order.
    #[must_use]
    pub fn tools(&self) -> Vec<&str> {
        self.tools.names()
    }

    /// Borrows the configured C4 inventory and router.
    #[must_use]
    pub const fn tool_components(&self) -> &ToolComponents {
        &self.tools
    }

    /// Mutably borrows the sole configured C4 router owner.
    #[must_use]
    pub const fn tool_components_mut(&mut self) -> &mut ToolComponents {
        &mut self.tools
    }
}

fn tool_error(error: ToolComponentError) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::InvalidInput,
        DaemonRecovery::CorrectRequest,
        "construct tool registry",
        error.to_string(),
        error,
    )
}

fn provider_error(error: peritus_provider_core::ProviderCoreError) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::InvalidInput,
        DaemonRecovery::CorrectRequest,
        "construct credential broker",
        error.to_string(),
        error,
    )
}
