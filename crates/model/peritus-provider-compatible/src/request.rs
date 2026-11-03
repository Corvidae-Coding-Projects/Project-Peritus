//! Fail-closed validation and private wire projection for both compatible dialects.

mod chat;
mod hosted;
mod responses;
mod validation;
mod value;

use peritus_model_protocol::{ModelRequest, WireDialect};
use peritus_provider_core::{
    Credential, Header, HeaderName, HttpHeaders, HttpMethod, HttpRequest, ProviderCoreError,
};

use crate::{CompatibleConfig, CompatibleProfile, error};

pub fn validate(
    profile: &CompatibleProfile,
    request: &ModelRequest,
) -> Result<(), ProviderCoreError> {
    validate_for_service(profile, request, None)
}

pub fn validate_for_service(
    profile: &CompatibleProfile,
    request: &ModelRequest,
    service: Option<peritus_provider_core::hosted::HostedService>,
) -> Result<(), ProviderCoreError> {
    validation::validate(profile, request, service)?;
    match profile.provider_profile().dialect() {
        WireDialect::CompatibleResponses => {
            if request.options().generation().seed().is_some()
                || !request.options().generation().stop_sequences().is_empty()
            {
                return Err(error::invalid(
                    "Responses compatibility does not map seed or stop sequences",
                ));
            }
        }
        WireDialect::CompatibleChatCompletions => {}
        _ => return Err(error::configuration("compatible profile dialect changed")),
    }
    Ok(())
}

pub fn encode(
    profile: &CompatibleProfile,
    request: &ModelRequest,
) -> Result<Vec<u8>, ProviderCoreError> {
    validate(profile, request)?;
    match profile.provider_profile().dialect() {
        WireDialect::CompatibleResponses => responses::encode(request),
        WireDialect::CompatibleChatCompletions => chat::encode(request),
        _ => Err(error::configuration("compatible profile dialect changed")),
    }
}

pub fn http_request(
    config: &CompatibleConfig,
    profile: &CompatibleProfile,
    request: &ModelRequest,
    credential: Credential,
) -> Result<HttpRequest, ProviderCoreError> {
    let body = if profile.provider_profile().dialect() == WireDialect::CompatibleChatCompletions
        && config.hosted_service().is_some()
    {
        validate_for_service(profile, request, config.hosted_service())?;
        chat::encode_hosted(request, config.hosted_service())?
    } else {
        encode(profile, request)?
    };
    let mut values = vec![
        config.auth().project(credential)?,
        Header::new(HeaderName::new("content-type".to_owned())?, b"application/json".to_vec())?,
        Header::new(HeaderName::new("accept".to_owned())?, b"text/event-stream".to_vec())?,
    ];
    for header in config.fixed_headers() {
        values.push(header.project()?);
    }
    let headers = HttpHeaders::new(values, config.http_limits())?;
    HttpRequest::new(
        HttpMethod::Post,
        config.endpoint().clone(),
        headers,
        body,
        config.http_limits(),
    )
}

#[cfg(test)]
mod policy_projection {
    use peritus_model_protocol::{OutputLimitEnforcement, WireDialect};
    use serde_json::Value;

    use crate::provider_policy_test_support as support;

    fn encode(dialect: WireDialect) -> Value {
        let profile =
            support::profile("compatible", dialect, OutputLimitEnforcement::ProviderEnforced);
        let request = support::request(&profile);
        let profile = match dialect {
            WireDialect::CompatibleResponses => {
                crate::CompatibleProfile::responses(profile).expect("compatible Responses profile")
            }
            WireDialect::CompatibleChatCompletions => {
                crate::CompatibleProfile::chat_completions(profile)
                    .expect("compatible Chat profile")
            }
            _ => unreachable!("fixture selects a compatible dialect"),
        };
        serde_json::from_slice(
            &super::encode(&profile, &request).expect("encode compatible request"),
        )
        .expect("parse compatible request")
    }

    #[test]
    fn system_policy_remains_a_system_message_in_both_compatible_dialects() {
        let responses = encode(WireDialect::CompatibleResponses);
        assert_eq!(responses["input"][0]["role"], "system");
        assert_eq!(responses["input"][0]["content"][0]["text"], support::POLICY_TEXT);
        assert_eq!(responses["input"][1]["role"], "user");
        assert_eq!(responses["input"][1]["content"][0]["text"], support::TASK_TEXT);
        assert_eq!(support::count(&responses.to_string(), support::POLICY_MARKER), 1);

        let chat = encode(WireDialect::CompatibleChatCompletions);
        assert_eq!(chat["messages"][0]["role"], "system");
        assert_eq!(chat["messages"][0]["content"][0]["text"], support::POLICY_TEXT);
        assert_eq!(chat["messages"][1]["role"], "user");
        assert_eq!(chat["messages"][1]["content"][0]["text"], support::TASK_TEXT);
        assert_eq!(support::count(&chat.to_string(), support::POLICY_MARKER), 1);
    }
}
