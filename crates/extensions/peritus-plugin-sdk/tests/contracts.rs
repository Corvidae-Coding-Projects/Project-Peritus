//! Canonical G3 plugin SDK contract and framing acceptance tests.

use peritus_plugin_sdk::{
    HostRequest, JsonBounds, JsonPayload, PluginManifest, PluginQuotas, PluginRequestEnvelope,
    PluginVersion, RequestId, SdkErrorKind, decode_frame, encode_frame,
};
use serde_json::Value;

fn json(text: &str) -> Value {
    serde_json::from_str(text).expect("test JSON")
}

const MANIFEST: &str = r#"
manifest_version = 1
id = "corvidae.echo"
kind = "process"

[version]
major = 1
minor = 2
patch = 3

[protocol]
minimum = 1
maximum = 1

[entrypoint]
artifact = "bin/echo-plugin"
arguments = ["--stdio"]

[[capabilities]]
name = "fs.read"
operation = "inspection"
required = true

[[capabilities]]
name = "quality.run"
operation = "execution"
required = false

[quotas]
concurrent_requests = 4
frame_bytes = 65536
output_bytes = 32768
invocation_millis = 30000
lifecycle_requests = 1000
protocol_violations = 2
"#;

#[test]
fn canonical_manifest_and_trust_material_are_stable() {
    let first = PluginManifest::parse_toml(MANIFEST).expect("manifest parses");
    let second = PluginManifest::parse_toml(MANIFEST).expect("manifest parses again");

    assert_eq!(first.id().as_str(), "corvidae.echo");
    assert_eq!(first.version(), PluginVersion::new(1, 2, 3));
    assert_eq!(
        first.canonical_bytes().expect("canonical bytes"),
        second.canonical_bytes().unwrap()
    );
    assert_eq!(first.digest().expect("digest"), second.digest().unwrap());

    let one = first.trust_material([1; 32]).expect("trust material").signature_preimage();
    let two = first.trust_material([2; 32]).expect("trust material").signature_preimage();
    assert_ne!(one, two);
}

#[test]
fn duplicate_capability_names_are_rejected_even_when_other_fields_differ() {
    let duplicate = MANIFEST.replace(
        "name = \"quality.run\"\noperation = \"execution\"\nrequired = false",
        "name = \"fs.read\"\noperation = \"workspace-mutation\"\nrequired = false",
    );
    let error = PluginManifest::parse_toml(&duplicate).expect_err("duplicate name rejected");
    assert_eq!(error.kind(), SdkErrorKind::NonCanonical);
}

#[test]
fn unsafe_entrypoints_and_unknown_manifest_fields_are_rejected() {
    let traversal = MANIFEST.replace("bin/echo-plugin", "../echo-plugin");
    assert_eq!(
        PluginManifest::parse_toml(&traversal).expect_err("traversal rejected").kind(),
        SdkErrorKind::InvalidManifest
    );

    let unknown = format!("{MANIFEST}\nunknown = true\n");
    assert_eq!(
        PluginManifest::parse_toml(&unknown).expect_err("unknown field rejected").kind(),
        SdkErrorKind::InvalidManifest
    );
}

#[test]
fn host_quota_intersection_never_widens_a_manifest() {
    let requested = PluginManifest::parse_toml(MANIFEST).expect("manifest").quotas();
    let ceiling = PluginQuotas {
        concurrent_requests: 2,
        frame_bytes: 1_024,
        output_bytes: 2_048,
        invocation_millis: 5_000,
        lifecycle_requests: 10,
        protocol_violations: 1,
    };
    assert_eq!(requested.narrow(ceiling), ceiling);
}

#[test]
fn payloads_are_canonical_and_recursively_bounded() {
    let payload = JsonPayload::new(json(r#"{"z":3,"a":[true,1]}"#), JsonBounds::PRODUCTION)
        .expect("bounded payload");
    assert_eq!(payload.canonical_bytes(), br#"{"a":[true,1],"z":3}"#);

    let float = JsonPayload::new(json(r#"{"value":1.5}"#), JsonBounds::PRODUCTION)
        .expect_err("floating point is not canonical");
    assert_eq!(float.kind(), SdkErrorKind::InvalidJson);

    let tiny = JsonBounds { max_bytes: 8, ..JsonBounds::PRODUCTION };
    assert_eq!(
        JsonPayload::new(json(r#"{"long":"value"}"#), tiny).expect_err("byte bound").kind(),
        SdkErrorKind::LimitExceeded
    );
}

#[test]
fn framed_protocol_roundtrips_and_rejects_trailing_or_oversized_input() {
    let envelope = PluginRequestEnvelope {
        protocol_version: 1,
        request_id: RequestId::new("request-1").expect("request id"),
        request: HostRequest::Health,
    };
    let frame = encode_frame(&envelope, 4_096).expect("frame");
    let decoded: PluginRequestEnvelope = decode_frame(&frame, 4_096).expect("decode");
    assert_eq!(decoded, envelope);

    let mut trailing = frame.clone();
    trailing.push(0);
    assert_eq!(
        decode_frame::<PluginRequestEnvelope>(&trailing, 4_096)
            .expect_err("trailing byte rejected")
            .kind(),
        SdkErrorKind::InvalidFrame
    );
    assert_eq!(
        decode_frame::<PluginRequestEnvelope>(&frame, 1)
            .expect_err("declared length exceeds limit")
            .kind(),
        SdkErrorKind::LimitExceeded
    );
}
