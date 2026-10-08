//! Checked provider-neutral request to private stable-v1 Google JSON projection.

mod content;
mod generate;
mod interactions;
mod value;

use peritus_model_protocol::{Capability, ContentBlock, ModelRequest, WireDialect};
use peritus_provider_core::{Endpoint, ProviderCoreError};

pub struct EncodedRequest {
    pub endpoint: Endpoint,
    pub body: Vec<u8>,
    pub structured: bool,
}

pub fn encode(
    request: &ModelRequest,
    base: &Endpoint,
) -> Result<EncodedRequest, ProviderCoreError> {
    validate(request)?;
    let value = match request.dialect() {
        WireDialect::GeminiInteractionsV1 => interactions::project(request)?,
        WireDialect::GeminiGenerateContentV1 => generate::project(request)?,
        _ => return Err(invalid("request selected a non-Google wire dialect")),
    };
    let endpoint = endpoint(request, base)?;
    let body = serde_json::to_vec(&value).map_err(|_| {
        ProviderCoreError::invalid_request("google_encode", "Google request serialization failed")
    })?;
    Ok(EncodedRequest {
        endpoint,
        body,
        structured: !matches!(
            request.options().output(),
            peritus_model_protocol::StructuredOutput::Text
        ),
    })
}

fn validate(request: &ModelRequest) -> Result<(), ProviderCoreError> {
    if !request.negotiated().includes(Capability::Streaming) {
        return Err(invalid(
            "Google streaming must be selected because the adapter opens a streaming response",
        ));
    }
    if request.options().persistence().background() {
        return Err(invalid(
            "foreground Google streams do not expose background retrieval or server cancellation",
        ));
    }
    if !request.options().extensions().is_empty() {
        return Err(invalid("Google provider extensions are not profile-authorized"));
    }
    if request.dialect() == WireDialect::GeminiGenerateContentV1
        && request.options().generation().stop_sequences().len() > 5
    {
        return Err(invalid("Google Generate Content accepts at most five stop sequences"));
    }
    if request.tools().iter().any(|tool| !valid_function_name(tool.name().as_str()))
        || request.messages().iter().flat_map(|message| message.content()).any(|block| {
            matches!(block, ContentBlock::ToolCall(call) if !valid_function_name(call.name().as_str()))
        })
    {
        return Err(invalid(
            "Google function names exceed the stable-v1 128-character grammar",
        ));
    }
    let model = request.model().as_str();
    if model.is_empty()
        || model.len() > 256
        || !model
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(invalid("Google model name is not safe for a stable-v1 request path"));
    }
    Ok(())
}

fn valid_function_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b':' | b'.' | b'-')
        })
}

fn endpoint(request: &ModelRequest, base: &Endpoint) -> Result<Endpoint, ProviderCoreError> {
    match request.dialect() {
        WireDialect::GeminiInteractionsV1 => base.with_path("/v1/interactions"),
        WireDialect::GeminiGenerateContentV1 => Endpoint::new(format!(
            "{}v1/models/{}:streamGenerateContent?alt=sse",
            base.as_str(),
            request.model().as_str()
        )),
        _ => Err(invalid("request selected a non-Google wire dialect")),
    }
}

pub const fn invalid(detail: &'static str) -> ProviderCoreError {
    ProviderCoreError::invalid_request("google_request", detail)
}

#[cfg(test)]
#[path = "request_tests.rs"]
mod tests;
