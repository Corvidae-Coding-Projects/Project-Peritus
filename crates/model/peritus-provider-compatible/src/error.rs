//! Redaction-safe compatible-adapter errors and normalized failures.

use peritus_model_protocol::{
    FailureCategory, ModelFailure, OutcomeCertainty, ProviderName, RedactedDiagnostic, ResponseId,
    Retryability, TransportPhase,
};
use peritus_provider_core::ProviderCoreError;

pub const fn configuration(detail: &'static str) -> ProviderCoreError {
    ProviderCoreError::configuration("compatible_config", detail)
}

pub const fn invalid(detail: &'static str) -> ProviderCoreError {
    ProviderCoreError::invalid_request("compatible_request", detail)
}

pub const fn malformed(detail: &'static str) -> ProviderCoreError {
    ProviderCoreError::malformed_stream("compatible_stream", detail)
}

pub const fn limit(detail: &'static str) -> ProviderCoreError {
    ProviderCoreError::limit_exceeded("compatible_stream", detail)
}

#[allow(
    clippy::too_many_arguments,
    reason = "provider failure classification has independent safety dimensions"
)]
pub fn failure(
    provider: &ProviderName,
    category: FailureCategory,
    phase: TransportPhase,
    certainty: OutcomeCertainty,
    retryability: Retryability,
    status: Option<u16>,
    response_id: Option<ResponseId>,
    retry_after_millis: Option<u64>,
    code: &'static str,
) -> Result<ModelFailure, ProviderCoreError> {
    let diagnostic = RedactedDiagnostic::new(code.to_owned(), None, None, None)
        .map_err(|_| malformed("static compatible diagnostic code was invalid"))?;
    Ok(ModelFailure::new(
        provider.clone(),
        category,
        phase,
        certainty,
        retryability,
        status,
        response_id,
        retry_after_millis,
        diagnostic,
    ))
}
