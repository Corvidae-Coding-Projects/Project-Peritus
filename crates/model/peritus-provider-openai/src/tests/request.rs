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
