use peritus_model_protocol::Capability;

use super::support::{fixture, minimal_request, profile_full, profile_minimal, realistic_request};

#[test]
fn minimal_request_matches_the_official_contract_golden() {
    let profile = profile_minimal();
    let request = minimal_request(&profile);
    let encoded = crate::request::encode(&request).expect("request encodes");
    let expected = fixture("golden-minimal-request.json");
    assert_eq!(encoded, expected.strip_suffix(b"\n").unwrap_or(&expected));
    let value: serde_json::Value = serde_json::from_slice(&encoded).expect("JSON");
    assert_eq!(value["stream"], true);
    assert_eq!(value["store"], false);
    assert!(value.get("seed").is_none());
    assert!(value.get("stop").is_none());
}

#[test]
fn realistic_request_projects_every_supported_request_family() {
    let profile = profile_full();
    let request = realistic_request(&profile);
    let encoded = crate::request::encode(&request).expect("request encodes");
    let expected = fixture("golden-realistic-request.json");
    assert_eq!(encoded, expected.strip_suffix(b"\n").unwrap_or(&expected));
}

#[test]
fn exact_reasoning_efforts_reach_the_api_body_and_survive_canonical_replay() {
    use peritus_model_protocol::{ProtocolLimits, ReasoningEffort as Effort, decode_request};
    for effort in [
        Effort::Minimal,
        Effort::Low,
        Effort::Medium,
        Effort::High,
        Effort::XHigh,
        Effort::Max,
        Effort::Ultra,
    ] {
        let profile = profile_full();
        let request = super::support::realistic_request_with_effort(&profile, effort);
        let encoded = crate::request::encode(&request).expect("encode effort");
        let value: serde_json::Value = serde_json::from_slice(&encoded).expect("body");
        assert_eq!(
            value.pointer("/reasoning/effort").and_then(serde_json::Value::as_str),
            Some(effort.as_str())
        );
        let bytes = request.canonical_bytes().expect("canonical");
        assert_eq!(
            decode_request(
                &bytes,
                &profile,
                request.request_id().clone(),
                ProtocolLimits::PRODUCTION
            )
            .expect("replay"),
            request
        );
    }
}

#[test]
fn nonnegotiated_streaming_fails_before_encoding() {
    let profile = profile_minimal();
    let request = super::support::request_with_capabilities(&profile, &[]);
    let error = crate::request::encode(&request).expect_err("streaming was not negotiated");
    assert_eq!(error.kind(), peritus_provider_core::ProviderCoreErrorKind::InvalidRequest);
    assert!(!request.negotiated().includes(Capability::Streaming));
}

#[test]
fn opencode_responses_preserves_gateway_path_and_stateless_wire_contract() {
    use crate::{
        OpenAiConfig,
        request::{RequestPlan, http_request},
    };
    use peritus_provider_core::{Credential, Endpoint};
    for endpoint in
        ["https://opencode.ai/zen/v1/responses", "https://opencode.ai/zen/go/v1/responses"]
    {
        let config = OpenAiConfig::opencode_gateway(
            Endpoint::new(endpoint.to_owned()).expect("endpoint"),
            super::support::credential_reference(),
        )
        .expect("gateway");
        let profile = profile_minimal();
        let request = minimal_request(&profile);
        let http = http_request(
            &config,
            &request,
            &RequestPlan::Create,
            Credential::new(b"fixture-key".to_vec()).expect("credential"),
        )
        .expect("request");
        assert_eq!(http.endpoint().as_str(), endpoint);
        let wire: serde_json::Value = serde_json::from_slice(http.body()).expect("wire");
        assert_eq!(wire["stream"], true);
        assert_eq!(wire["store"], false);
        assert!(wire.get("input").is_some());
        assert!(wire.get("messages").is_none());
    }
    assert!(
        OpenAiConfig::opencode_gateway(
            Endpoint::new("https://unrelated.invalid/v1/responses".to_owned()).expect("endpoint"),
            super::support::credential_reference()
        )
        .is_err()
    );
}

#[test]
fn initial_reasoning_request_includes_encrypted_state_before_any_replay_exists() {
    let profile = profile_full();
    let request = super::support::request_with_capabilities(
        &profile,
        &[Capability::Streaming, Capability::ReasoningControls],
    );
    let wire: serde_json::Value =
        serde_json::from_slice(&crate::request::encode(&request).expect("request")).expect("wire");
    assert_eq!(wire["include"], serde_json::json!(["reasoning.encrypted_content"]));
    assert!(wire.get("reasoning").is_none());
}
