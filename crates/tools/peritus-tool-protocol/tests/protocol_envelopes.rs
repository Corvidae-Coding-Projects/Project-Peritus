//! Byte-stable schema, descriptor, call, progress, artifact, result, and error fixtures.

use std::sync::Arc;

use peritus_policy::{AuthorityInstant, OperationClass, OperationDescriptor, RiskClass, RiskSet};
use peritus_tool_protocol::{
    ArtifactCompleteness, ArtifactProvenance, ArtifactReference, BoundedJson, BoundedText,
    CallLimits, CancellationReason, CanonicalEnvelope, ControlSet, FailureCategory, IdempotencyKey,
    IdempotencySemantics, ImplementationIdentity, JsonLimits, LeaseRequirement, ProgressKind,
    ProtocolCompatibility, ProtocolError, ProtocolErrorKind, RecoveryRoute, ResponsibleSubsystem,
    Retryability, Schema, SchemaCompatibility, SchemaProperty, SemanticVersion, SideEffectClass,
    ToolCall, ToolControl, ToolDescriptor, ToolFailure, ToolLimits, ToolProgress, ToolResult,
    ToolTiming, Truncation, TruncationMetadata, prepare_call,
};
use peritus_types::{
    AcceptanceSpecId, ActionId, CapabilityName, Generation, HarnessId, PolicyId, ProviderProfileId,
    RevisionNumber, RevisionTuple, Sha256Digest, WorkspaceId,
};

fn revision() -> RevisionTuple {
    RevisionTuple::new(
        AcceptanceSpecId::new([1; 16]).unwrap(),
        HarnessId::new([2; 16]).unwrap(),
        WorkspaceId::new([3; 16]).unwrap(),
        Generation::new(4).unwrap(),
        RevisionNumber::new(5).unwrap(),
        PolicyId::new([6; 16]).unwrap(),
        ProviderProfileId::new([7; 16]).unwrap(),
    )
}

fn schema() -> Schema {
    Schema::object(
        vec![
            SchemaProperty::new(
                "count".to_owned(),
                Schema::integer(Some(0), Some(9)).unwrap(),
                true,
            )
            .unwrap(),
            SchemaProperty::new("label".to_owned(), Schema::string(1, 24).unwrap(), false).unwrap(),
        ],
        false,
    )
    .unwrap()
}

fn descriptor() -> Arc<ToolDescriptor> {
    let name = CapabilityName::new("fixture.inspect".to_owned()).unwrap();
    let operation = OperationDescriptor::new(
        name.clone(),
        OperationClass::Inspection,
        RiskSet::new(vec![RiskClass::Read]).unwrap(),
    )
    .unwrap();
    Arc::new(
        ToolDescriptor::new(
            name,
            SemanticVersion::new(1, 2, 3).unwrap(),
            schema(),
            operation,
            SideEffectClass::None,
            LeaseRequirement::None,
            IdempotencySemantics::ReplayTerminal,
            ImplementationIdentity::new("fixture:0.0.0:p1:catalog".to_owned()).unwrap(),
            ToolLimits::new(5_000, 65_536, 2_048, 4_096, 16, 4, 1_024).unwrap(),
            ControlSet::new(false, false, false, true, true),
            ProtocolCompatibility::V1,
            BoundedText::new("Fixture inspection".to_owned()).unwrap(),
        )
        .unwrap(),
    )
}

fn prepared() -> peritus_tool_protocol::PreparedToolCall {
    let arguments =
        BoundedJson::parse(r#"{"label":"x","count":3}"#, JsonLimits::PRODUCTION).unwrap();
    let call = ToolCall::new(
        ActionId::new([8; 16]).unwrap(),
        CapabilityName::new("fixture.inspect".to_owned()).unwrap(),
        SemanticVersion::new(1, 2, 3).unwrap(),
        arguments,
        CallLimits::new(2_000, 4_096, 512, 1_024, 8, 2).unwrap(),
        revision(),
        AuthorityInstant::new(Generation::first(), 100),
        IdempotencyKey::new("fixture-key".to_owned()).unwrap(),
    );
    prepare_call(descriptor(), call).unwrap()
}

#[test]
fn canonical_json_and_schema_fixtures_are_byte_stable() {
    let value = BoundedJson::parse(
        r#"{ "z": [true, null], "a": {"two":2,"one":1} }"#,
        JsonLimits::PRODUCTION,
    )
    .unwrap();
    assert_eq!(value.canonical_bytes(), br#"{"a":{"one":1,"two":2},"z":[true,null]}"#,);
    let round_trip = BoundedJson::parse(
        std::str::from_utf8(value.canonical_bytes()).unwrap(),
        JsonLimits::PRODUCTION,
    )
    .unwrap();
    assert_eq!(round_trip, value);
    assert_eq!(
        schema().canonical_bytes(),
        br#"{"additionalProperties":false,"properties":{"count":{"maximum":9,"minimum":0,"type":"integer"},"label":{"maxLength":24,"minLength":1,"type":"string"}},"required":["count"],"type":"object"}"#,
    );
}

#[test]
fn descriptor_and_prepared_generation_is_deterministic() {
    let first = descriptor();
    let second = descriptor();
    assert_eq!(first.canonical_bytes(), second.canonical_bytes());
    assert_eq!(first.descriptor_digest(), second.descriptor_digest());
    let first = prepared();
    let second = prepared();
    assert_eq!(first.canonical_bytes(), second.canonical_bytes());
    assert_eq!(first.prepared_digest(), second.prepared_digest());
    assert_eq!(first.replay_identity(), second.replay_identity());
}

#[test]
fn schema_compatibility_allows_only_additive_optional_properties() {
    let current = schema();
    assert_eq!(current.compatibility_with(&current), SchemaCompatibility::Equal);
    let additive = Schema::object(
        vec![
            SchemaProperty::new(
                "count".to_owned(),
                Schema::integer(Some(0), Some(9)).unwrap(),
                true,
            )
            .unwrap(),
            SchemaProperty::new("extra".to_owned(), Schema::boolean(), false).unwrap(),
            SchemaProperty::new("label".to_owned(), Schema::string(1, 24).unwrap(), false).unwrap(),
        ],
        false,
    )
    .unwrap();
    assert_eq!(current.compatibility_with(&additive), SchemaCompatibility::Additive);
    let required = Schema::object(
        vec![
            SchemaProperty::new(
                "count".to_owned(),
                Schema::integer(Some(0), Some(9)).unwrap(),
                true,
            )
            .unwrap(),
            SchemaProperty::new("extra".to_owned(), Schema::boolean(), true).unwrap(),
            SchemaProperty::new("label".to_owned(), Schema::string(1, 24).unwrap(), false).unwrap(),
        ],
        false,
    )
    .unwrap();
    assert_eq!(current.compatibility_with(&required), SchemaCompatibility::Breaking);
}

#[test]
fn complete_schema_validation_rejects_wrong_and_extra_values() {
    let wrong = BoundedJson::parse(r#"{"count":10}"#, JsonLimits::PRODUCTION).unwrap();
    assert_eq!(schema().validate(&wrong).unwrap_err().kind(), ProtocolErrorKind::SchemaViolation);
    let extra = BoundedJson::parse(r#"{"count":1,"extra":true}"#, JsonLimits::PRODUCTION).unwrap();
    assert_eq!(schema().validate(&extra).unwrap_err().kind(), ProtocolErrorKind::SchemaViolation);
    let missing = BoundedJson::parse(r#"{"label":"x"}"#, JsonLimits::PRODUCTION).unwrap();
    assert_eq!(schema().validate(&missing).unwrap_err().kind(), ProtocolErrorKind::SchemaViolation);
}

#[test]
fn canonical_json_rejects_duplicate_keys_at_every_depth() {
    let error =
        BoundedJson::parse(r#"{"outer":{"same":1,"same":2}}"#, JsonLimits::PRODUCTION).unwrap_err();
    assert_eq!(error.kind(), ProtocolErrorKind::InvalidJson);
    assert_eq!(error.detail(), "JSON object contains a duplicate key");
}

#[test]
fn json_limit_construction_can_only_narrow_production_ceilings() {
    assert!(JsonLimits::new(1024, 8, 64, 256).is_ok());
    assert!(
        JsonLimits::new(
            JsonLimits::PRODUCTION.max_bytes() + 1,
            JsonLimits::PRODUCTION.max_depth(),
            JsonLimits::PRODUCTION.max_members(),
            JsonLimits::PRODUCTION.max_string_bytes(),
        )
        .is_err(),
    );
}

#[test]
fn every_public_envelope_has_stable_versioned_bytes() {
    let prepared = prepared();
    let progress = ToolProgress::new(
        &prepared,
        0,
        ProgressKind::Started,
        AuthorityInstant::new(Generation::first(), 20),
        None,
        BoundedText::new("started".to_owned()).unwrap(),
    )
    .unwrap();
    let provenance =
        ArtifactProvenance::new(prepared.call().action_id(), prepared.prepared_digest());
    let artifact = ArtifactReference::new(
        Sha256Digest::new([9; 32]),
        12,
        BoundedText::new("text/plain".to_owned()).unwrap(),
        BoundedText::new("stdout".to_owned()).unwrap(),
        ArtifactCompleteness::Complete,
        provenance,
    )
    .unwrap();
    let timing = ToolTiming::new(
        AuthorityInstant::new(Generation::first(), 20),
        AuthorityInstant::new(Generation::first(), 21),
    )
    .unwrap();
    let result = ToolResult::success(
        &prepared,
        BoundedJson::null(),
        BoundedText::new("ok".to_owned()).unwrap(),
        BoundedText::new("ok".to_owned()).unwrap(),
        vec![artifact.clone()],
        timing,
        TruncationMetadata {
            output: Truncation::Complete,
            model: Truncation::Complete,
            human: Truncation::Complete,
        },
        1,
    )
    .unwrap();
    let error = ProtocolError::invalid_envelope("fixture".to_owned(), "fixture failure").unwrap();
    let failure = ToolFailure::new(
        FailureCategory::Execution,
        BoundedText::new("fixture_failed".to_owned()).unwrap(),
        ResponsibleSubsystem::Tool,
        Retryability::Never,
        RecoveryRoute::None,
        BoundedText::new("fixture failure".to_owned()).unwrap(),
    );
    let control = ToolControl::Cancel(CancellationReason::Requested);
    for bytes in [
        prepared.call().canonical_bytes(),
        prepared.canonical_bytes(),
        progress.canonical_bytes(),
        artifact.canonical_bytes(),
        result.canonical_bytes(),
        failure.canonical_bytes(),
        error.canonical_bytes(),
        control.canonical_bytes(),
    ] {
        let decoded = CanonicalEnvelope::parse(&bytes, 1024 * 1024).unwrap();
        assert_eq!(decoded.version(), 1);
        assert!(!decoded.payload().is_empty());
        assert_eq!(decoded.canonical_bytes(), bytes);
    }
}

#[test]
fn common_envelope_decoder_enforces_transport_bounds_and_header() {
    let bytes = prepared().canonical_bytes();
    assert!(CanonicalEnvelope::parse(&bytes, bytes.len() - 1).is_err());
    let mut wrong_magic = bytes;
    wrong_magic[0] = b'X';
    assert!(CanonicalEnvelope::parse(&wrong_magic, 1024 * 1024).is_err());
}

#[test]
fn progress_and_terminal_output_enforce_narrowed_call_bounds() {
    let prepared = prepared();
    let oversized_progress = ToolProgress::new(
        &prepared,
        0,
        ProgressKind::Update,
        AuthorityInstant::new(Generation::first(), 20),
        None,
        BoundedText::new("p".repeat(513)).unwrap(),
    );
    assert!(oversized_progress.is_err());

    let oversized_output = BoundedJson::string("o".repeat(4_097), JsonLimits::PRODUCTION).unwrap();
    let result = ToolResult::success(
        &prepared,
        oversized_output,
        BoundedText::new("failure".to_owned()).unwrap(),
        BoundedText::new("failure".to_owned()).unwrap(),
        Vec::new(),
        ToolTiming::new(
            AuthorityInstant::new(Generation::first(), 20),
            AuthorityInstant::new(Generation::first(), 21),
        )
        .unwrap(),
        TruncationMetadata {
            output: Truncation::Complete,
            model: Truncation::Complete,
            human: Truncation::Complete,
        },
        0,
    );
    assert!(result.is_err());
}

#[test]
fn protocol_error_rejects_an_over_limit_path() {
    let error = ProtocolError::invalid_envelope("x".repeat(8 * 1024 + 1), "failure").unwrap_err();
    assert_eq!(error.kind(), ProtocolErrorKind::InvalidText);
    assert!(error.path().len() <= 8 * 1024);
    assert!(error.canonical_bytes().len() < 8 * 1024);
}

#[test]
fn recursive_schema_error_paths_are_utf8_safely_bounded() {
    let long_key = "é".repeat(5_000);
    let input = format!(r#"{{"{long_key}":true}}"#);
    let value = BoundedJson::parse(&input, JsonLimits::PRODUCTION).unwrap();
    let error = Schema::object(Vec::new(), false).unwrap().validate(&value).unwrap_err();

    assert_eq!(error.kind(), ProtocolErrorKind::SchemaViolation);
    assert!(error.path().len() <= 8 * 1024);
    assert!(error.path().ends_with("...[truncated]"));
    assert!(std::str::from_utf8(error.path().as_bytes()).is_ok());
}
