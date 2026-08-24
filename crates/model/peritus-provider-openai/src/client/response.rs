//! Bounded response-body handling and normalized pre-stream failures.

use peritus_model_protocol::{
    FailureCategory, ModelEvent, OutcomeCertainty, ProviderProfile, Retryability, TransportPhase,
};
use peritus_provider_core::{
    ByteStream, CancellationToken, HttpHeaders, OwnedModelStream, ProviderCoreError,
};

use crate::{error, stream::OpenAiStream};

pub(super) async fn read_body(
    body: &mut Box<dyn ByteStream>,
    cancellation: &CancellationToken,
    maximum: usize,
) -> Result<Vec<u8>, ProviderCoreError> {
    let mut bytes = Vec::new();
    while let Some(chunk) = body.next(cancellation).await? {
        if bytes.len().checked_add(chunk.len()).is_none_or(|length| length > maximum) {
            return Err(ProviderCoreError::limit_exceeded(
                "openai_error_body",
                "OpenAI response body exceeds its byte bound",
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

pub(super) fn connection_failure(
    profile: &ProviderProfile,
    cancellation: &CancellationToken,
) -> Result<OwnedModelStream, ProviderCoreError> {
    failure_stream(
        profile,
        cancellation,
        FailureCategory::Transport,
        TransportPhase::Connecting,
        OutcomeCertainty::DefinitelyNotAccepted,
        Retryability::SafeNewRequest,
        "openai.connect.failed",
    )
}

pub(super) fn ambiguous_failure(
    profile: &ProviderProfile,
    cancellation: &CancellationToken,
) -> Result<OwnedModelStream, ProviderCoreError> {
    failure_stream(
        profile,
        cancellation,
        FailureCategory::AmbiguousAcceptance,
        TransportPhase::SendingBody,
        OutcomeCertainty::MaybeAccepted,
        Retryability::CallerDecision,
        "openai.submission.ambiguous",
    )
}

pub(super) fn add_request_bytes(total: u64, body_bytes: usize) -> Result<u64, ProviderCoreError> {
    total
        .checked_add(u64::try_from(body_bytes).map_err(|_| {
            ProviderCoreError::limit_exceeded(
                "openai_retry",
                "OpenAI request byte count cannot be represented",
            )
        })?)
        .ok_or_else(|| {
            ProviderCoreError::limit_exceeded(
                "openai_retry",
                "OpenAI cumulative request byte count overflowed",
            )
        })
}

pub(super) fn is_event_stream(headers: &HttpHeaders) -> bool {
    headers
        .first("content-type")
        .and_then(|value| value.nonsensitive_bytes())
        .and_then(|bytes| core::str::from_utf8(bytes).ok())
        .is_some_and(|value| {
            value.split(';').next().is_some_and(|media_type| {
                media_type.trim().eq_ignore_ascii_case("text/event-stream")
            })
        })
}

pub(super) fn failure_stream(
    profile: &ProviderProfile,
    cancellation: &CancellationToken,
    category: FailureCategory,
    phase: TransportPhase,
    certainty: OutcomeCertainty,
    retryability: Retryability,
    code: &'static str,
) -> Result<OwnedModelStream, ProviderCoreError> {
    let failure = error::failure(
        profile.provider(),
        category,
        phase,
        certainty,
        retryability,
        None,
        None,
        None,
        code,
    )?;
    let stream = OpenAiStream::failure_stream(
        profile.provider().clone(),
        ModelEvent::ResponseFailed(failure),
        peritus_codec::sha256(code.as_bytes()),
    )?;
    Ok(OwnedModelStream::new(stream, cancellation.clone()))
}
