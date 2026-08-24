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
/// Rejects provider/dialect drift, non-streaming profiles, unsupported lifecycle guarantees, or
/// features that Anthropic Messages cannot implement through this adapter.
pub fn validate_anthropic_profile(profile: &ProviderProfile) -> Result<(), ProviderCoreError> {
    let capabilities = profile.capabilities();
    if profile.provider().as_str() != "anthropic"
        || profile.dialect() != WireDialect::AnthropicMessages
        || profile.output_limit_enforcement() != OutputLimitEnforcement::ProviderEnforced
        || profile.state_mode() != StateMode::StatelessReplay
        || profile.resume_kind() != ResumeKind::Unsupported
        || profile.cancellation_kind() != CancellationKind::BestEffortLocalAbort
        || !capabilities.supports(Capability::Streaming)
        || [
            Capability::AudioInput,
            Capability::ResumableResponse,
            Capability::ConfirmedCancellation,
            Capability::StoredState,
            Capability::ProviderExtensions,
        ]
        .into_iter()
        .any(|capability| capabilities.supports(capability))
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
