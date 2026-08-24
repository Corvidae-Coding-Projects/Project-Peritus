//! Fail-closed validation and private wire projection for both compatible dialects.

mod chat;
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
    validation::validate(profile, request)?;
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
    let body = encode(profile, request)?;
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
