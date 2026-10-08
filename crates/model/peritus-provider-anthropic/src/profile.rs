//! Exact Anthropic Messages profile and lifecycle validation.

use peritus_model_protocol::{
    CancellationKind, Capability, OutputLimitEnforcement, ProviderProfile, ResumeKind, StateMode,
    WireDialect,
};
use peritus_provider_core::ProviderCoreError;

/// Validates the immutable profile assumptions implemented by this adapter.
///
/// # Errors
///
/// Rejects provider/dialect drift, non-streaming profiles, or unsupported lifecycle guarantees.
/// Feature-specific mappings are admitted against the exact request that selects them.
pub fn validate_anthropic_profile(profile: &ProviderProfile) -> Result<(), ProviderCoreError> {
    let capabilities = profile.capabilities();
    if profile.provider().as_str() != "anthropic"
        || profile.dialect() != WireDialect::AnthropicMessages
        || profile.output_limit_enforcement() != OutputLimitEnforcement::ProviderEnforced
        || profile.state_mode() != StateMode::StatelessReplay
        || profile.resume_kind() != ResumeKind::Unsupported
        || profile.cancellation_kind() != CancellationKind::BestEffortLocalAbort
        || !capabilities.supports(Capability::Streaming)
    {
        return Err(ProviderCoreError::configuration(
            "anthropic_profile",
            "profile contradicts the exact Anthropic Messages dialect or lifecycle",
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "profile_tests.rs"]
mod tests;
