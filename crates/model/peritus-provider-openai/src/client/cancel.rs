//! Provider-confirmed cancellation for known stored background responses.

use peritus_model_protocol::{
    CanonicalJson, Capability, JsonBounds, ProtocolLimits, ResponseId, StateMode,
};
use peritus_provider_core::{
    BoxFuture, ByteStream, CancellationToken, Header, HeaderName, HttpHeaders, HttpMethod,
    HttpRequest, ProviderCoreError, ResponseCancellationOutcome,
};
use serde_json::Value;

use super::OpenAiProvider;

pub(super) fn cancel<'a>(
    provider: &'a OpenAiProvider,
    response_id: &'a ResponseId,
    cancellation: &'a CancellationToken,
) -> BoxFuture<'a, Result<ResponseCancellationOutcome, ProviderCoreError>> {
    Box::pin(async move {
        if provider.profile.state_mode() != StateMode::BackgroundResumable
            || !provider.profile.capabilities().supports(Capability::ConfirmedCancellation)
        {
            return Ok(ResponseCancellationOutcome::Unsupported);
        }
        require_known(provider, response_id)?;
        if cancellation.is_cancelled() {
            return Err(ProviderCoreError::cancelled("openai_cancel"));
        }
        let credential = provider.credentials.resolve(provider.config.credential())?;
        let endpoint = cancel_endpoint(provider, response_id)?;
        let headers = cancel_headers(provider, credential)?;
        let request = HttpRequest::new(
            HttpMethod::Post,
            endpoint,
            headers,
            Vec::new(),
            provider.config.http_limits(),
        )?;
        let response = provider.transport.send(request, cancellation).await?;
        let (status, _headers, mut body) = response.into_parts();
        if status.as_u16() != 200 {
            return Err(cancel_status(status.as_u16()));
        }
        let value = read_native_outcome(
            &mut body,
            cancellation,
            provider.config.http_limits().max_response_body_bytes(),
        )
        .await?;
        let identity = value.get("id").and_then(serde_json::Value::as_str);
        let state = value.get("status").and_then(serde_json::Value::as_str);
        if identity != Some(response_id.expose_for_wire()) {
            return Err(ProviderCoreError::malformed_stream(
                "openai_cancel",
                "OpenAI cancellation response identity changed",
            ));
        }
        let already_terminal = match state {
            Some("cancelled") => false,
            Some("completed" | "failed" | "incomplete") => true,
            _ => {
                return Err(ProviderCoreError::malformed_stream(
                    "openai_cancel",
                    "OpenAI cancellation response had an invalid status",
                ));
            }
        };
        forget_known(provider, response_id)?;
        Ok(ResponseCancellationOutcome::Confirmed { already_terminal })
    })
}

async fn read_native_outcome(
    body: &mut Box<dyn ByteStream>,
    cancellation: &CancellationToken,
    maximum: usize,
) -> Result<Value, ProviderCoreError> {
    let mut bytes = Vec::new();
    loop {
        if let Some(value) = complete_native_outcome(&bytes)? {
            return Ok(value);
        }
        if bytes.len() == maximum {
            return Err(ProviderCoreError::limit_exceeded(
                "openai_cancel",
                "OpenAI cancellation response exceeds its byte bound",
            ));
        }
        let Some(chunk) = body.next(cancellation).await? else {
            return Err(ProviderCoreError::malformed_stream(
                "openai_cancel",
                "OpenAI cancellation response ended before its native status",
            ));
        };
        if bytes
            .len()
            .checked_add(chunk.len())
            .is_none_or(|length| length > maximum)
        {
            return Err(ProviderCoreError::limit_exceeded(
                "openai_cancel",
                "OpenAI cancellation response exceeds its byte bound",
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
}

fn complete_native_outcome(bytes: &[u8]) -> Result<Option<Value>, ProviderCoreError> {
    let mut values = serde_json::Deserializer::from_slice(bytes).into_iter::<Value>();
    let value = match values.next() {
        None => return Ok(None),
        Some(Err(failure)) if failure.is_eof() => return Ok(None),
        Some(Err(_)) => {
            return Err(ProviderCoreError::malformed_stream(
                "openai_cancel",
                "OpenAI cancellation response was malformed",
            ));
        }
        Some(Ok(value)) => value,
    };
    if !value.is_object() {
        return Err(ProviderCoreError::malformed_stream(
            "openai_cancel",
            "OpenAI cancellation response was not a JSON object",
        ));
    }
    let consumed = values.byte_offset();
    let prefix = core::str::from_utf8(&bytes[..consumed]).map_err(|_| {
        ProviderCoreError::malformed_stream(
            "openai_cancel",
            "OpenAI cancellation response was not UTF-8",
        )
    })?;
    CanonicalJson::parse(prefix, JsonBounds::value(ProtocolLimits::PRODUCTION)).map_err(|_| {
        ProviderCoreError::malformed_stream(
            "openai_cancel",
            "OpenAI cancellation response was recursively unbounded",
        )
    })?;
    Ok(Some(value))
}

fn require_known(
    provider: &OpenAiProvider,
    response_id: &ResponseId,
) -> Result<(), ProviderCoreError> {
    provider.background_responses.require_active(response_id)
}

fn forget_known(
    provider: &OpenAiProvider,
    response_id: &ResponseId,
) -> Result<(), ProviderCoreError> {
    provider.background_responses.retire_cancelled(response_id)
}

fn cancel_endpoint(
    provider: &OpenAiProvider,
    response_id: &ResponseId,
) -> Result<peritus_provider_core::Endpoint, ProviderCoreError> {
    let identity = response_id.expose_for_wire();
    if !identity.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')) {
        return Err(ProviderCoreError::invalid_request(
            "openai_cancel",
            "OpenAI response identity is unsafe in a cancellation path",
        ));
    }
    provider.config.endpoint().with_path(&format!("/v1/responses/{identity}/cancel"))
}

fn cancel_headers(
    provider: &OpenAiProvider,
    credential: peritus_provider_core::Credential,
) -> Result<HttpHeaders, ProviderCoreError> {
    let mut headers = vec![
        credential.into_header(HeaderName::new("authorization".to_owned())?, Some("Bearer "))?,
        Header::new(HeaderName::new("accept".to_owned())?, b"application/json".to_vec())?,
    ];
    if let Some(value) = provider.config.organization() {
        headers
            .push(Header::new(HeaderName::new("openai-organization".to_owned())?, value.to_vec())?);
    }
    if let Some(value) = provider.config.project() {
        headers.push(Header::new(HeaderName::new("openai-project".to_owned())?, value.to_vec())?);
    }
    HttpHeaders::new(headers, provider.config.http_limits())
}

const fn cancel_status(status: u16) -> ProviderCoreError {
    match status {
        400 | 404 | 409 | 422 => ProviderCoreError::invalid_request(
            "openai_cancel",
            "OpenAI rejected the background cancellation request",
        ),
        401 | 403 => ProviderCoreError::credential(
            "OpenAI rejected the credential used for background cancellation",
        ),
        429 | 500..=599 => ProviderCoreError::transport(
            "openai_cancel",
            "OpenAI cancellation endpoint was temporarily unavailable",
        ),
        _ => ProviderCoreError::transport(
            "openai_cancel",
            "OpenAI cancellation endpoint returned an unsupported status",
        ),
    }
}
