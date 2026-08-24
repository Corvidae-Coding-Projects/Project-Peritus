//! Exact stable-v1 Google dialect and lifecycle validation.

use peritus_model_protocol::{
    CancellationKind, Capability, OutputLimitEnforcement, ProviderProfile, ResumeKind, StateMode,
    WireDialect,
};
use peritus_provider_core::ProviderCoreError;

/// Validates the immutable profile assumptions implemented by this adapter.
///
/// # Errors
///
/// Rejects provider/version drift, unsupported lifecycle guarantees, or a capability claim that
/// contradicts the selected stable-v1 dialect.
pub fn validate_google_profile(profile: &ProviderProfile) -> Result<(), ProviderCoreError> {
    let capabilities = profile.capabilities();
    let common = profile.provider().as_str() == "google"
        && profile.output_limit_enforcement() == OutputLimitEnforcement::ProviderEnforced
        && capabilities.supports(Capability::Streaming)
        && profile.cancellation_kind() == CancellationKind::BestEffortLocalAbort
        && !capabilities.supports(Capability::ConfirmedCancellation)
        && !capabilities.supports(Capability::ProviderExtensions);
    let lifecycle = match profile.dialect() {
        WireDialect::GeminiInteractionsV1 => match profile.state_mode() {
            StateMode::StatelessReplay => {
                profile.resume_kind() == ResumeKind::Unsupported
                    && !capabilities.supports(Capability::StoredState)
                    && !capabilities.supports(Capability::ResumableResponse)
            }
            StateMode::ProviderStored => {
                profile.resume_kind() == ResumeKind::SemanticContinuation
                    && capabilities.supports(Capability::StoredState)
                    && capabilities.supports(Capability::ResumableResponse)
            }
            StateMode::BackgroundResumable => false,
        },
        WireDialect::GeminiGenerateContentV1 => {
            profile.state_mode() == StateMode::StatelessReplay
                && profile.resume_kind() == ResumeKind::Unsupported
                && !capabilities.supports(Capability::StoredState)
                && !capabilities.supports(Capability::ResumableResponse)
        }
        _ => false,
    };
    if !common || !lifecycle {
        return Err(ProviderCoreError::configuration(
            "google_profile",
            "profile contradicts a stable-v1 Google dialect or its implemented lifecycle",
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "profile_tests.rs"]
mod tests;
