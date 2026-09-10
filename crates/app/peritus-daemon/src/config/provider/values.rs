//! Closed capability names, retry policy, and safe configuration diagnostics.
use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};
use peritus_model_protocol::Capability;
use peritus_provider_core::RetryPolicy;
use std::time::Duration;

pub(super) fn capability(value: &str) -> Result<Capability, DaemonError> {
    match value {
        "streaming" => Ok(Capability::Streaming),
        "tool-calls" => Ok(Capability::ToolCalls),
        "parallel-tool-calls" => Ok(Capability::ParallelToolCalls),
        "strict-structured-output" => Ok(Capability::StrictStructuredOutput),
        "prompt-caching" => Ok(Capability::PromptCaching),
        "image-input" => Ok(Capability::ImageInput),
        "audio-input" => Ok(Capability::AudioInput),
        "document-input" => Ok(Capability::DocumentInput),
        "reasoning-replay" => Ok(Capability::ReasoningReplay),
        "reasoning-controls" => Ok(Capability::ReasoningControls),
        "reasoning-summaries" => Ok(Capability::ReasoningSummaries),
        "resumable-response" => Ok(Capability::ResumableResponse),
        "confirmed-cancellation" => Ok(Capability::ConfirmedCancellation),
        "usage-detail" => Ok(Capability::UsageDetail),
        "rate-limit-detail" => Ok(Capability::RateLimitDetail),
        "stored-state" => Ok(Capability::StoredState),
        "provider-extensions" => Ok(Capability::ProviderExtensions),
        "sampling-controls" => Ok(Capability::SamplingControls),
        _ => Err(invalid("provider profile contains an unknown capability name")),
    }
}

pub(super) fn retry_policy() -> Result<RetryPolicy, DaemonError> {
    RetryPolicy::new(
        3,
        [
            Duration::from_millis(100),
            Duration::from_secs(2),
            Duration::from_secs(2),
            Duration::from_secs(10),
        ],
        64 * 1024 * 1024,
    )
    .map_err(provider_error)
}

pub(super) fn provider_error(error: peritus_provider_core::ProviderCoreError) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::InvalidInput,
        DaemonRecovery::CorrectRequest,
        "construct provider route",
        error.to_string(),
        error,
    )
}

pub(super) fn protocol_error(error: peritus_model_protocol::ProtocolError) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::InvalidInput,
        DaemonRecovery::CorrectRequest,
        "construct provider profile",
        error.to_string(),
        error,
    )
}

pub(super) fn invalid(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::InvalidInput,
        DaemonRecovery::CorrectRequest,
        "validate daemon provider inventory",
        detail,
    )
}
