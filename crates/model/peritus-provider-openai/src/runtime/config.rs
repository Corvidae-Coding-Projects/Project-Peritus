//! Immutable Codex runtime configuration and narrow capability validation.

use core::fmt;

use peritus_model_protocol::{
    CancellationKind, Capability, CapabilityState, OutputLimitEnforcement, ProviderProfile,
    ResumeKind, StateMode, WireDialect,
};
use peritus_provider_core::{ProcessLimits, ProviderCoreError};

use super::CodexExecutable;

/// Immutable configuration for one constrained Codex executable provider.
#[derive(Clone)]
pub struct CodexRuntimeConfig {
    executable: CodexExecutable,
    profile: ProviderProfile,
    process_limits: ProcessLimits,
}

impl CodexRuntimeConfig {
    /// Binds a pinned executable to one honest account-runtime profile.
    ///
    /// # Errors
    ///
    /// Rejects provider, dialect, lifecycle, output-limit, or capability drift.
    pub fn new(
        executable: CodexExecutable,
        profile: ProviderProfile,
        process_limits: ProcessLimits,
    ) -> Result<Self, ProviderCoreError> {
        validate(&profile)?;
        Ok(Self { executable, profile, process_limits })
    }

    /// Returns the pinned Codex executable.
    #[must_use]
    pub const fn executable(&self) -> &CodexExecutable {
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

impl fmt::Debug for CodexRuntimeConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CodexRuntimeConfig")
            .field("executable", &self.executable)
            .field("profile", &self.profile)
            .field("process_limits", &self.process_limits)
            .finish()
    }
}

fn validate(profile: &ProviderProfile) -> Result<(), ProviderCoreError> {
    let capabilities = profile.capabilities();
    let exact = capabilities.iter().all(|(capability, state)| {
        state != CapabilityState::Unknown
            && (state != CapabilityState::Supported
                || matches!(
                    capability,
                    Capability::ToolCalls | Capability::ParallelToolCalls | Capability::UsageDetail
                ))
    });
    if profile.provider().as_str() != "openai"
        || profile.dialect() != WireDialect::OpenAiCodexRuntime
        || profile.output_limit_enforcement() != OutputLimitEnforcement::Advisory
        || profile.state_mode() != StateMode::StatelessReplay
        || profile.resume_kind() != ResumeKind::Unsupported
        || profile.cancellation_kind() != CancellationKind::BestEffortLocalAbort
        || !exact
        || capabilities.supports(Capability::ParallelToolCalls)
            && !capabilities.supports(Capability::ToolCalls)
    {
        return Err(ProviderCoreError::configuration(
            "codex_runtime_profile",
            "profile contradicts the constrained Codex executable runtime",
        ));
    }
    Ok(())
}
