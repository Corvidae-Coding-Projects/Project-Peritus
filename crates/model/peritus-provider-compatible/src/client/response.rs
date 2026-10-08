use peritus_model_protocol::{
    FailureCategory, ModelEvent, OutcomeCertainty, Retryability, TransportPhase,
};
use peritus_provider_core::{
    ByteStream, CancellationToken, HttpHeaders, OwnedModelStream, ProviderCoreError,
};

use crate::{CompatibleProfile, error, stream::CompatibleStream};

pub(super) async fn read_body(
    body: &mut Box<dyn ByteStream>,
    cancellation: &CancellationToken,
    maximum: usize,
) -> Result<Vec<u8>, ProviderCoreError> {
    let mut bytes = Vec::new();
    while let Some(chunk) = body.next(cancellation).await? {
        if bytes.len().checked_add(chunk.len()).is_none_or(|length| length > maximum) {
            return Err(error::limit("compatible response body exceeded its byte bound"));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

pub(super) fn is_event_stream(headers: &HttpHeaders) -> bool {
    headers
        .first("content-type")
        .and_then(|value| value.nonsensitive_bytes())
        .and_then(|bytes| core::str::from_utf8(bytes).ok())
        .is_some_and(|value| {
            value
                .split(';')
                .next()
                .is_some_and(|media| media.trim().eq_ignore_ascii_case("text/event-stream"))
        })
}

fn add_request_bytes(total: u64, length: usize) -> Result<u64, ProviderCoreError> {
    let length = u64::try_from(length)
        .map_err(|_| error::limit("compatible request length was not representable"))?;
    total
        .checked_add(length)
        .ok_or_else(|| error::limit("compatible cumulative request bytes overflowed"))
}

pub(super) fn admit_request_bytes(
    total: u64,
    length: usize,
    maximum: u64,
) -> Result<u64, ProviderCoreError> {
    let next = add_request_bytes(total, length)?;
    if next > maximum {
        return Err(error::limit(
            "compatible encoded request exceeded its explicit finite retry byte budget",
        ));
    }
    Ok(next)
}

pub(super) fn can_admit_request_bytes(
    total: u64,
    length: usize,
    maximum: u64,
) -> Result<bool, ProviderCoreError> {
    Ok(add_request_bytes(total, length)? <= maximum)
}

pub(super) fn failure_stream(
    profile: &CompatibleProfile,
    cancellation: &CancellationToken,
    category: FailureCategory,
    phase: TransportPhase,
    certainty: OutcomeCertainty,
    retryability: Retryability,
    code: &'static str,
) -> Result<OwnedModelStream, ProviderCoreError> {
    let failure = error::failure(
        profile.provider_profile().provider(),
        category,
        phase,
        certainty,
        retryability,
        None,
        None,
        None,
        code,
    )?;
    let stream = CompatibleStream::failure_stream(
        profile.provider_profile().provider().clone(),
        ModelEvent::ResponseFailed(failure),
        peritus_codec::sha256(code.as_bytes()),
    )?;
    Ok(OwnedModelStream::new(stream, cancellation.clone()))
}
