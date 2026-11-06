//! Checked provider-neutral request to private stable-v1 Google JSON projection.

mod content;
mod generate;
mod interactions;
mod value;

use peritus_model_protocol::{Capability, ModelRequest, WireDialect};
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
mod policy_projection {
    use peritus_model_protocol::{OutputLimitEnforcement, WireDialect};
    use peritus_provider_core::Endpoint;
    use serde_json::Value;

    use crate::provider_policy_test_support as support;

    fn encode(dialect: WireDialect) -> Value {
        let profile = support::profile("google", dialect, OutputLimitEnforcement::ProviderEnforced);
        let request = support::request(&profile);
        let endpoint =
            Endpoint::new("https://fixture.invalid/".to_owned()).expect("fixture Google endpoint");
        let encoded = super::encode(&request, &endpoint).expect("encode Google request");
        serde_json::from_slice(&encoded.body).expect("parse Google request")
    }

    #[test]
    fn system_policy_remains_elevated_in_both_google_dialects() {
        let interactions = encode(WireDialect::GeminiInteractionsV1);
        assert_eq!(interactions["system_instruction"], support::POLICY_TEXT);
        assert_eq!(interactions["input"][0]["type"], "user_input");
        assert_eq!(interactions["input"][0]["content"][0]["text"], support::TASK_TEXT);
        assert_eq!(support::count(&interactions.to_string(), support::POLICY_MARKER), 1);

        let generate = encode(WireDialect::GeminiGenerateContentV1);
        assert_eq!(generate["systemInstruction"]["parts"][0]["text"], support::POLICY_TEXT);
        assert_eq!(generate["contents"][0]["role"], "user");
        assert_eq!(generate["contents"][0]["parts"][0]["text"], support::TASK_TEXT);
        assert_eq!(support::count(&generate.to_string(), support::POLICY_MARKER), 1);
    }
}
