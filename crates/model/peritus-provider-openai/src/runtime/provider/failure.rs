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
    usage: peritus_model_protocol::UsageCounters,
) -> Result<CodexRuntimeStream, ProviderCoreError> {
    let (category, retryability, code) = match reason {
        DecodeFailure::Authentication => (
            FailureCategory::Authentication,
            Retryability::Never,
            "openai.codex_runtime.authentication",
        ),
        DecodeFailure::Safety => {
            (FailureCategory::Safety, Retryability::Never, "openai.codex_runtime.safety")
        }
        DecodeFailure::RateLimited => (
            FailureCategory::RateLimited,
            Retryability::SafeNewRequest,
            "openai.codex_runtime.rate_limited",
        ),
        DecodeFailure::Capacity => (
            FailureCategory::TransientProvider,
            Retryability::SafeNewRequest,
            "openai.codex_runtime.capacity",
        ),
        DecodeFailure::QuotaExhausted => (
            FailureCategory::QuotaExhausted,
            Retryability::Never,
            "openai.codex_runtime.quota_exhausted",
        ),
        DecodeFailure::ContextLimit => (
            FailureCategory::InvalidRequest,
            Retryability::Never,
            "openai.codex_runtime.context_limit",
        ),
        DecodeFailure::Reported => {
            (FailureCategory::Provider, Retryability::Never, "openai.codex_runtime.reported")
        }
        DecodeFailure::Incomplete => (
            FailureCategory::IncompleteStream,
            Retryability::Never,
            "openai.codex_runtime.incomplete",
        ),
        DecodeFailure::NativeTool => {
            (FailureCategory::Safety, Retryability::Never, decoding_code(reason))
        }
        DecodeFailure::Malformed
        | DecodeFailure::InvalidLifecycle
        | DecodeFailure::InvalidEnvelope
        | DecodeFailure::InvalidToolArguments
        | DecodeFailure::InvalidToolChoice
        | DecodeFailure::OutputLimit
        | DecodeFailure::InvalidUsage
        | DecodeFailure::MultipleMessages
        | DecodeFailure::UnsupportedEvent => {
            (FailureCategory::MalformedPayload, Retryability::CallerDecision, decoding_code(reason))
        }
    };
    let (phase, certainty, partial) = match (reason, category) {
        (DecodeFailure::NativeTool, _) | (_, FailureCategory::MalformedPayload) => {
            (TransportPhase::ReadingBody, OutcomeCertainty::MaybeAccepted, false)
        }
        (_, FailureCategory::IncompleteStream) => {
            (TransportPhase::StreamObserved, OutcomeCertainty::AcceptedPartial, true)
        }
        (
            _,
            FailureCategory::RateLimited
            | FailureCategory::TransientProvider
            | FailureCategory::InvalidRequest,
        ) => (TransportPhase::Completed, OutcomeCertainty::DefinitelyNotAccepted, false),
        _ => (TransportPhase::Completed, OutcomeCertainty::Terminal, false),
    };
    CodexRuntimeStream::failed_observed(
        model,
        failure(provider, category, phase, certainty, retryability, code, None)?,
        b"openai-codex-runtime-decoding",
        partial,
        usage,
    )
}

const fn decoding_code(reason: &DecodeFailure) -> &'static str {
    match reason {
        DecodeFailure::Malformed => "openai.codex_runtime.invalid_jsonl",
        DecodeFailure::InvalidLifecycle => "openai.codex_runtime.invalid_lifecycle",
        DecodeFailure::InvalidEnvelope => "openai.codex_runtime.invalid_envelope",
        DecodeFailure::InvalidToolArguments => "openai.codex_runtime.invalid_tool_arguments",
        DecodeFailure::InvalidToolChoice => "openai.codex_runtime.invalid_tool_choice",
        DecodeFailure::OutputLimit => "openai.codex_runtime.output_limit",
        DecodeFailure::InvalidUsage => "openai.codex_runtime.invalid_usage",
        DecodeFailure::MultipleMessages => "openai.codex_runtime.multiple_messages",
        DecodeFailure::UnsupportedEvent => "openai.codex_runtime.unsupported_event",
        DecodeFailure::NativeTool => "openai.codex_runtime.native_tool",
        _ => "openai.codex_runtime.malformed",
    }
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
