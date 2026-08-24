//! Provider-confirmed cancellation for known stored background responses.

use peritus_model_protocol::{
    CanonicalJson, Capability, JsonBounds, ProtocolLimits, ResponseId, StateMode,
};
use peritus_provider_core::{
    BoxFuture, CancellationToken, Header, HeaderName, HttpHeaders, HttpMethod, HttpRequest,
    ProviderCoreError, ResponseCancellationOutcome,
};

use super::{OpenAiProvider, response};

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
        let bytes = response::read_body(
            &mut body,
            cancellation,
            provider.config.http_limits().max_response_body_bytes(),
        )
        .await?;
        if status.as_u16() != 200 {
            return Err(cancel_status(status.as_u16()));
        }
        CanonicalJson::parse(
            core::str::from_utf8(&bytes).map_err(|_| {
                ProviderCoreError::malformed_stream(
                    "openai_cancel",
                    "OpenAI cancellation response was not UTF-8",
                )
            })?,
            JsonBounds::value(ProtocolLimits::PRODUCTION),
        )
        .map_err(|_| {
            ProviderCoreError::malformed_stream(
                "openai_cancel",
                "OpenAI cancellation response was malformed or unbounded",
            )
        })?;
        let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| {
            ProviderCoreError::malformed_stream(
                "openai_cancel",
                "OpenAI cancellation response was not a JSON object",
            )
        })?;
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

fn require_known(
    provider: &OpenAiProvider,
    response_id: &ResponseId,
) -> Result<(), ProviderCoreError> {
    let known = provider.resumable_background.lock().map_err(|_| {
        ProviderCoreError::invalid_request(
            "openai_cancel",
            "OpenAI background-response registry is unavailable",
        )
    })?;
    if !known.contains(response_id) {
        return Err(ProviderCoreError::invalid_request(
            "openai_cancel",
            "provider cancellation requires a background response observed by this adapter",
        ));
    }
    drop(known);
    Ok(())
}

fn forget_known(
    provider: &OpenAiProvider,
    response_id: &ResponseId,
) -> Result<(), ProviderCoreError> {
    provider
        .resumable_background
        .lock()
        .map_err(|_| {
            ProviderCoreError::invalid_request(
                "openai_cancel",
                "OpenAI background-response registry is unavailable",
            )
        })?
        .remove(response_id);
    Ok(())
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
