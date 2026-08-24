//! Immutable Claude runtime configuration and narrow capability validation.

use core::fmt;

use peritus_model_protocol::{
    CancellationKind, Capability, CapabilityState, OutputLimitEnforcement, ProviderProfile,
    ResumeKind, StateMode, WireDialect,
};
use peritus_provider_core::{ProcessLimits, ProviderCoreError};

use super::ClaudeExecutable;

/// Immutable configuration for one constrained Claude executable provider.
#[derive(Clone)]
pub struct ClaudeRuntimeConfig {
    executable: ClaudeExecutable,
    profile: ProviderProfile,
    process_limits: ProcessLimits,
}

impl ClaudeRuntimeConfig {
    /// Binds a pinned executable to one honest account-runtime profile.
    ///
    /// # Errors
    ///
    /// Rejects provider, dialect, lifecycle, output-limit, or capability drift.
    pub fn new(
        executable: ClaudeExecutable,
        profile: ProviderProfile,
        process_limits: ProcessLimits,
    ) -> Result<Self, ProviderCoreError> {
        validate(&profile)?;
        Ok(Self { executable, profile, process_limits })
    }

    /// Returns the pinned Anthropic executable.
    #[must_use]
    pub const fn executable(&self) -> &ClaudeExecutable {
        &self.executable
    }

    /// Returns the immutable provider profile.
    #[must_use]
    pub const fn profile(&self) -> &ProviderProfile {
        &self.profile
    }

    /// Returns turn process ceilings.
    #[must_use]
    pub const fn process_limits(&self) -> ProcessLimits {
        self.process_limits
    }
}

impl fmt::Debug for ClaudeRuntimeConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClaudeRuntimeConfig")
            .field("executable", &self.executable)
            .field("profile", &self.profile)
            .field("process_limits", &self.process_limits)
            .finish()
    }
}

fn validate(profile: &ProviderProfile) -> Result<(), ProviderCoreError> {
    let capabilities = profile.capabilities();
    let supported_are_exact = capabilities.iter().all(|(capability, state)| {
        state != CapabilityState::Unknown
            && (state != CapabilityState::Supported
                || matches!(
                    capability,
                    Capability::ToolCalls | Capability::ParallelToolCalls | Capability::UsageDetail
                ))
    });
    if profile.provider().as_str() != "anthropic"
        || profile.dialect() != WireDialect::AnthropicClaudeRuntime
        || profile.output_limit_enforcement() != OutputLimitEnforcement::Advisory
        || profile.state_mode() != StateMode::StatelessReplay
        || profile.resume_kind() != ResumeKind::Unsupported
        || profile.cancellation_kind() != CancellationKind::BestEffortLocalAbort
        || !supported_are_exact
        || capabilities.supports(Capability::ParallelToolCalls)
            && !capabilities.supports(Capability::ToolCalls)
    {
        return Err(ProviderCoreError::configuration(
            "claude_runtime_profile",
            "profile contradicts the constrained Claude executable runtime",
        ));
    }
    Ok(())
}
