//! Redacted Anthropic status, stream, and transport failure classification.

use peritus_model_protocol::{
    FailureCategory, ModelFailure, OutcomeCertainty, ProviderName, RedactedDiagnostic, ResponseId,
    Retryability, TransportPhase,
};
use peritus_provider_core::ProviderCoreError;

pub fn status_failure(
    provider: ProviderName,
    status: u16,
    retry_after_millis: Option<u64>,
    quota_hint: bool,
    response_id: Option<ResponseId>,
) -> Result<ModelFailure, ProviderCoreError> {
    let (category, retryability, code) = match status {
        400 | 413 => (FailureCategory::InvalidRequest, Retryability::Never, "anthropic.request"),
        401 => (FailureCategory::Authentication, Retryability::Never, "anthropic.auth"),
        402 => (FailureCategory::QuotaExhausted, Retryability::Never, "anthropic.billing"),
        403 => (FailureCategory::Permission, Retryability::Never, "anthropic.permission"),
        404 => (FailureCategory::NotFound, Retryability::Never, "anthropic.not_found"),
        429 if quota_hint => {
            (FailureCategory::QuotaExhausted, Retryability::Never, "anthropic.quota")
        }
        429 => (FailureCategory::RateLimited, Retryability::SafeNewRequest, "anthropic.rate_limit"),
        409 | 500 | 504 | 529 => (
            FailureCategory::TransientProvider,
            Retryability::SafeNewRequest,
            "anthropic.transient",
        ),
        _ => (FailureCategory::Provider, Retryability::Never, "anthropic.provider"),
    };
    failure(
        provider,
        FailureFacts {
            category,
            phase: TransportPhase::ReadingBody,
            certainty: OutcomeCertainty::DefinitelyNotAccepted,
            retryability,
            status: Some(status),
            response_id,
            retry_after_millis,
            code,
        },
    )
}

pub fn ambiguous_transport(provider: ProviderName) -> Result<ModelFailure, ProviderCoreError> {
    failure(
        provider,
        FailureFacts {
            category: FailureCategory::AmbiguousAcceptance,
            phase: TransportPhase::SendingBody,
            certainty: OutcomeCertainty::MaybeAccepted,
            retryability: Retryability::CallerDecision,
            status: None,
            response_id: None,
            retry_after_millis: None,
            code: "anthropic.ambiguous",
        },
    )
}

pub fn stream_failure(
    provider: ProviderName,
    category: FailureCategory,
    observed: bool,
    code: &'static str,
) -> Result<ModelFailure, ProviderCoreError> {
    failure(
        provider,
        FailureFacts {
            category,
            phase: if observed {
                TransportPhase::StreamObserved
            } else {
                TransportPhase::ReadingBody
            },
            certainty: if observed {
                OutcomeCertainty::AcceptedPartial
            } else {
                OutcomeCertainty::MaybeAccepted
            },
            retryability: Retryability::Never,
            status: Some(200),
            response_id: None,
            retry_after_millis: None,
            code,
        },
    )
}

#[derive(Clone)]
struct FailureFacts {
    category: FailureCategory,
    phase: TransportPhase,
    certainty: OutcomeCertainty,
    retryability: Retryability,
    status: Option<u16>,
    response_id: Option<ResponseId>,
    retry_after_millis: Option<u64>,
    code: &'static str,
}

fn failure(provider: ProviderName, facts: FailureFacts) -> Result<ModelFailure, ProviderCoreError> {
    let diagnostic =
        RedactedDiagnostic::new(facts.code.to_owned(), None, None, None).map_err(|_| {
            ProviderCoreError::configuration(
                "anthropic_error",
                "static Anthropic diagnostic construction failed",
            )
        })?;
    Ok(ModelFailure::new(
        provider,
        facts.category,
        facts.phase,
        facts.certainty,
        facts.retryability,
        facts.status,
        facts.response_id,
        facts.retry_after_millis,
        diagnostic,
    ))
}

#[cfg(test)]
#[path = "error_tests.rs"]
mod tests;
