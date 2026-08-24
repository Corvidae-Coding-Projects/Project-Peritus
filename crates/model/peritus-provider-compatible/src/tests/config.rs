use peritus_model_protocol::Capability;
use peritus_provider_core::{Endpoint, HeaderName};

use super::support::{credential_reference, responses_profile};
use crate::{CompatibleAuth, CompatibleConfig, CompatibleProfile};

#[test]
fn dialect_and_capability_contracts_are_exact_and_fail_closed() {
    let responses = responses_profile(&[Capability::Streaming]);
    assert!(CompatibleProfile::responses(responses.clone()).is_ok());
    assert!(CompatibleProfile::chat_completions(responses).is_err());

    let unsupported = responses_profile(&[Capability::Streaming, Capability::ReasoningControls]);
    assert!(CompatibleProfile::responses(unsupported).is_err());
}

#[test]
fn config_requires_an_exact_operation_path_and_safe_header_auth() {
    let auth = CompatibleAuth::bearer(credential_reference()).expect("bearer");
    let root = Endpoint::new("https://example.invalid".to_owned()).expect("root endpoint");
    assert!(CompatibleConfig::new(root, auth).is_err());

    let unsafe_header = HeaderName::new("x-routing-id".to_owned()).expect("header");
    assert!(CompatibleAuth::raw_header(credential_reference(), unsafe_header).is_err());
    let api_key = HeaderName::new("x-api-key".to_owned()).expect("header");
    assert!(CompatibleAuth::raw_header(credential_reference(), api_key).is_ok());
}
