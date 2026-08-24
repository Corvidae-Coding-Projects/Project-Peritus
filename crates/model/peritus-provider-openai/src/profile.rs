//! Exact first-party Responses profile validation.

use peritus_model_protocol::{
    CancellationKind, Capability, CapabilityState, OutputLimitEnforcement, ProviderProfile,
    ResumeKind, StateMode, WireDialect,
};
use peritus_provider_core::ProviderCoreError;

pub fn validate(profile: &ProviderProfile) -> Result<(), ProviderCoreError> {
    if profile.provider().as_str() != "openai"
        || profile.dialect() != WireDialect::OpenAiResponses
        || profile.output_limit_enforcement() != OutputLimitEnforcement::ProviderEnforced
        || !profile.capabilities().supports(Capability::Streaming)
    {
        return Err(ProviderCoreError::invalid_request(
            "openai_profile",
            "profile is not an exact first-party OpenAI Responses streaming profile",
        ));
    }
    if profile.capabilities().state(Capability::ProviderExtensions) != CapabilityState::Unsupported
    {
        return Err(ProviderCoreError::invalid_request(
            "openai_profile",
            "profile claims an OpenAI behavior this adapter does not expose",
        ));
    }
    validate_state(profile)
}

fn validate_state(profile: &ProviderProfile) -> Result<(), ProviderCoreError> {
    let stored = profile.capabilities().supports(Capability::StoredState);
    let resumable = profile.capabilities().supports(Capability::ResumableResponse);
    let cancellation = profile.capabilities().state(Capability::ConfirmedCancellation);
    let valid_cancellation = if profile.state_mode() == StateMode::BackgroundResumable {
        matches!(
            (cancellation, profile.cancellation_kind()),
            (CapabilityState::Unsupported, CancellationKind::BestEffortLocalAbort)
                | (CapabilityState::Supported, CancellationKind::Confirmed)
        )
    } else {
        cancellation == CapabilityState::Unsupported
            && profile.cancellation_kind() == CancellationKind::BestEffortLocalAbort
    };
    let valid = valid_cancellation
        && match (stored, resumable) {
            (false, false) => {
                profile.state_mode() == StateMode::StatelessReplay
                    && profile.resume_kind() == ResumeKind::Unsupported
            }
            (true, false) => {
                profile.state_mode() == StateMode::ProviderStored
                    && profile.resume_kind() == ResumeKind::SemanticContinuation
            }
            (true, true) => {
                profile.state_mode() == StateMode::BackgroundResumable
                    && profile.resume_kind() == ResumeKind::ExactCursor
            }
            (false, true) => false,
        };
    if !valid {
        return Err(ProviderCoreError::invalid_request(
            "openai_profile",
            "profile storage and continuation lifecycle is inconsistent",
        ));
    }
    Ok(())
}
