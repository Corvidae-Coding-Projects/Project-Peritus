//! Final-process result classification and normalized stream construction.

use peritus_model_protocol::{
    FailureCategory, ModelRequest, OutcomeCertainty, ProviderName, TransportPhase,
};
use peritus_provider_core::{
    CancellationToken, OwnedModelStream, ProcessOutput, ProviderCoreError,
};

use super::super::output::{DecodeFailure, decode};
use super::super::request::RuntimeRequest;
use super::super::stream::ClaudeRuntimeStream;
use super::{failure, runtime_failure};

pub(super) fn normalize(
    request: &ModelRequest,
    runtime: &RuntimeRequest,
    output: &ProcessOutput,
    provider: ProviderName,
    cancellation: CancellationToken,
) -> Result<OwnedModelStream, ProviderCoreError> {
    if !output.exit().success() {
        let (category, certainty, code) = if output.stdout().is_empty() {
            (
                FailureCategory::AmbiguousAcceptance,
                OutcomeCertainty::MaybeAccepted,
                "anthropic.claude_runtime.process",
            )
        } else {
            (
                FailureCategory::IncompleteStream,
                OutcomeCertainty::AcceptedPartial,
                "anthropic.claude_runtime.process_interrupted",
            )
        };
        return failed(
            request,
            runtime_failure(provider, category, certainty, code)?,
            b"claude-runtime-process",
            cancellation,
        );
    }
    let turn = match decode(output.stdout(), &runtime.allowed_tools, runtime.max_calls) {
        Ok(turn) => turn,
        Err(error) => return decode_failure(request, provider, cancellation, error),
    };
    let stream = ClaudeRuntimeStream::completed(request, turn, output.stdout())?;
    Ok(OwnedModelStream::new(stream, cancellation))
}

fn decode_failure(
    request: &ModelRequest,
    provider: ProviderName,
    cancellation: CancellationToken,
    error: DecodeFailure,
) -> Result<OwnedModelStream, ProviderCoreError> {
    let (failure, digest) = match error {
        DecodeFailure::Authentication => (
            runtime_failure(
                provider,
                FailureCategory::Authentication,
                OutcomeCertainty::Terminal,
                "anthropic.claude_runtime.authentication",
            )?,
            b"claude-runtime-authentication".as_slice(),
        ),
        DecodeFailure::Capacity => (
            runtime_failure(
                provider,
                FailureCategory::TransientProvider,
                OutcomeCertainty::DefinitelyNotAccepted,
                "anthropic.claude_runtime.capacity",
            )?,
            b"claude-runtime-capacity".as_slice(),
        ),
        DecodeFailure::ContextLimit => (
            runtime_failure(
                provider,
                FailureCategory::InvalidRequest,
                OutcomeCertainty::DefinitelyNotAccepted,
                "anthropic.claude_runtime.context_limit",
            )?,
            b"claude-runtime-context-limit".as_slice(),
        ),
        DecodeFailure::Reported => (
            runtime_failure(
                provider,
                FailureCategory::Provider,
                OutcomeCertainty::Terminal,
                "anthropic.claude_runtime.result_error",
            )?,
            b"claude-runtime-result-error".as_slice(),
        ),
        DecodeFailure::Incomplete => (
            failure(
                provider,
                FailureCategory::IncompleteStream,
                TransportPhase::ReadingBody,
                OutcomeCertainty::AcceptedPartial,
                "anthropic.claude_runtime.incomplete",
            )?,
            b"claude-runtime-incomplete".as_slice(),
        ),
        DecodeFailure::Malformed => (
            runtime_failure(
                provider,
                FailureCategory::MalformedPayload,
                OutcomeCertainty::MaybeAccepted,
                "anthropic.claude_runtime.malformed",
            )?,
            b"claude-runtime-malformed".as_slice(),
        ),
    };
    failed(request, failure, digest, cancellation)
}

fn failed(
    request: &ModelRequest,
    failure: peritus_model_protocol::ModelFailure,
    digest: &'static [u8],
    cancellation: CancellationToken,
) -> Result<OwnedModelStream, ProviderCoreError> {
    let stream = ClaudeRuntimeStream::failed(request.model().clone(), failure, digest)?;
    Ok(OwnedModelStream::new(stream, cancellation))
}
