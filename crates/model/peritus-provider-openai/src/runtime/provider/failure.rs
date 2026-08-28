//! Redaction-safe runtime terminal classification.

use peritus_model_protocol::{
    FailureCategory, ModelFailure, ModelName, OutcomeCertainty, ProviderName, RedactedDiagnostic,
    Retryability, TransportPhase,
};
use peritus_provider_core::ProviderCoreError;

use super::super::output::DecodeFailure;
use super::super::stream::CodexRuntimeStream;

pub(super) fn authentication(provider: ProviderName) -> Result<ModelFailure, ProviderCoreError> {
    failure(
        provider,
        FailureCategory::Authentication,
        TransportPhase::BeforeSend,
        OutcomeCertainty::DefinitelyNotAccepted,
        Retryability::Never,
        "openai.codex_runtime.authentication",
        None,
    )
}

pub(super) fn decode_failure(
    model: ModelName,
    provider: ProviderName,
    reason: &DecodeFailure,
) -> Result<CodexRuntimeStream, ProviderCoreError> {
    let (category, phase, certainty, retryability, code, retry_after, partial) = match reason {
        DecodeFailure::Authentication => (
            FailureCategory::Authentication,
            TransportPhase::Completed,
            OutcomeCertainty::Terminal,
            Retryability::Never,
            "openai.codex_runtime.authentication",
            None,
            false,
        ),
        DecodeFailure::Reported => (
            FailureCategory::Provider,
            TransportPhase::Completed,
            OutcomeCertainty::Terminal,
            Retryability::Never,
            "openai.codex_runtime.reported",
            None,
            false,
        ),
        DecodeFailure::Incomplete => (
            FailureCategory::IncompleteStream,
            TransportPhase::StreamObserved,
            OutcomeCertainty::AcceptedPartial,
            Retryability::Never,
            "openai.codex_runtime.incomplete",
            None,
            true,
        ),
        DecodeFailure::Malformed | DecodeFailure::NativeTool => (
            FailureCategory::MalformedPayload,
            TransportPhase::ReadingBody,
            OutcomeCertainty::MaybeAccepted,
            Retryability::CallerDecision,
            "openai.codex_runtime.malformed",
            None,
            false,
        ),
    };
    CodexRuntimeStream::failed(
        model,
        failure(provider, category, phase, certainty, retryability, code, retry_after)?,
        b"openai-codex-runtime-decoding",
        partial,
    )
}

pub(super) fn failure(
    provider: ProviderName,
    category: FailureCategory,
    phase: TransportPhase,
    certainty: OutcomeCertainty,
    retryability: Retryability,
    code: &'static str,
    retry_after_millis: Option<u64>,
) -> Result<ModelFailure, ProviderCoreError> {
    let diagnostic = RedactedDiagnostic::new(code.to_owned(), None, None, None).map_err(|_| {
        ProviderCoreError::configuration(
            "codex_runtime_failure",
            "static Codex runtime diagnostic could not be constructed",
        )
    })?;
    Ok(ModelFailure::new(
        provider,
        category,
        phase,
        certainty,
        retryability,
        None,
        None,
        retry_after_millis,
        diagnostic,
    ))
}
