//! Strict model proposal schema, fixture, and digest coverage.

use peritus_debugger::{MODEL_PROPOSAL_SCHEMA, model_proposal_schema};
use peritus_model_protocol::{CanonicalJson, JsonBounds, ProtocolLimits, SchemaDialect};
use sha2::{Digest, Sha256};
use std::{fmt::Write as _, path::PathBuf};

#[test]
fn strict_schema_is_stable_parseable_and_closed() {
    let limits = ProtocolLimits::PRODUCTION;
    let schema = model_proposal_schema(SchemaDialect::Draft202012, limits)
        .expect("closed debugger proposal schema");
    assert_eq!(schema.dialect(), SchemaDialect::Draft202012);
    assert_eq!(
        schema.digest(),
        peritus_model_protocol::JsonSchema::parse(
            MODEL_PROPOSAL_SCHEMA,
            SchemaDialect::Draft202012,
            JsonBounds::schema(limits),
        )
        .expect("same schema")
        .digest(),
    );
    let schema_text = std::str::from_utf8(schema.canonical_bytes()).expect("schema is UTF-8");
    assert!(schema_text.contains("\"additionalProperties\":false"));
    assert!(schema_text.contains("\"affected_component_tags\""));
    assert!(!schema_text.contains("tool"));
}

#[test]
fn v1_proposal_fixtures_are_bounded_canonical_json() {
    let bounds = JsonBounds::value(ProtocolLimits::PRODUCTION);
    let valid_source = fixture("model_proposal_valid.json");
    let rejected_source = fixture("model_proposal_unknown_field.json");
    let valid = CanonicalJson::parse(&valid_source, bounds).expect("valid proposal JSON");
    let rejected =
        CanonicalJson::parse(&rejected_source, bounds).expect("well-formed negative JSON");
    assert_ne!(valid.digest(), rejected.digest());
    assert!(
        std::str::from_utf8(rejected.canonical_bytes())
            .expect("canonical JSON is UTF-8")
            .contains("\"authority\""),
        "the negative fixture exercises the closed-schema unknown-field rejection",
    );
}

#[test]
fn v1_fixture_digest_manifest_matches_exact_bytes() {
    let valid = fixture("model_proposal_valid.json");
    let unknown_field = fixture("model_proposal_unknown_field.json");
    let expected = [
        ("model_proposal_valid.json", valid.as_bytes()),
        ("model_proposal_unknown_field.json", unknown_field.as_bytes()),
    ];
    let manifest = fixture("SHA256SUMS");
    let declared = manifest.lines().collect::<Vec<_>>();
    assert_eq!(declared.len(), expected.len());
    for (name, bytes) in expected {
        let digest =
            Sha256::digest(bytes).iter().fold(String::with_capacity(64), |mut encoded, byte| {
                write!(&mut encoded, "{byte:02x}").expect("writing to a string cannot fail");
                encoded
            });
        assert!(
            declared.iter().any(|line| *line == format!("{digest}  {name}")),
            "fixture digest manifest is stale for {name}",
        );
    }
}

fn fixture(name: &str) -> String {
    let manifest_dir = std::env::var_os("CARGO_MANIFEST_DIR").expect("Cargo manifest directory");
    let path = PathBuf::from(manifest_dir).join("tests/fixtures/v1").join(name);
    std::fs::read_to_string(path).expect("immutable UTF-8 debugger fixture")
}
