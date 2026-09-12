use peritus_model_protocol::Capability;

use super::support::{
    chat_profile, fixture, minimal_request, request_with_capabilities, responses_profile,
};
use crate::CompatibleProfile;

#[test]
fn both_dialects_have_distinct_stable_golden_requests() {
    let responses = responses_profile(&[Capability::Streaming]);
    let responses_contract = CompatibleProfile::responses(responses.clone()).expect("profile");
    let encoded = crate::request::encode(&responses_contract, &minimal_request(&responses))
        .expect("Responses request");
    let expected = fixture("golden-responses-request.json");
    assert_eq!(encoded, expected.strip_suffix(b"\n").unwrap_or(&expected));

    let chat = chat_profile(&[Capability::Streaming]);
    let chat_contract = CompatibleProfile::chat_completions(chat.clone()).expect("profile");
    let encoded =
        crate::request::encode(&chat_contract, &minimal_request(&chat)).expect("Chat request");
    let expected = fixture("golden-chat-request.json");
    assert_eq!(encoded, expected.strip_suffix(b"\n").unwrap_or(&expected));
}

#[test]
fn chat_usage_projection_matches_request_negotiation() {
    let provider = chat_profile(&[Capability::Streaming, Capability::UsageDetail]);
    let profile = CompatibleProfile::chat_completions(provider.clone()).expect("profile");
    for usage in [false, true] {
        let mut capabilities = vec![Capability::Streaming];
        if usage {
            capabilities.push(Capability::UsageDetail);
        }
        let request = request_with_capabilities(&provider, &capabilities);
        let encoded = crate::request::encode(&profile, &request).expect("Chat request");
        let json: serde_json::Value = serde_json::from_slice(&encoded).expect("request JSON");
        assert_eq!(json["stream_options"]["include_usage"], usage);
    }
}

#[test]
fn streaming_is_rejected_before_wire_projection_when_not_negotiated() {
    let provider = responses_profile(&[Capability::Streaming]);
    let profile = CompatibleProfile::responses(provider.clone()).expect("profile");
    let request = request_with_capabilities(&provider, &[]);
    let error = crate::request::encode(&profile, &request).expect_err("must reject");
    assert_eq!(error.kind(), peritus_provider_core::ProviderCoreErrorKind::InvalidRequest);
}
