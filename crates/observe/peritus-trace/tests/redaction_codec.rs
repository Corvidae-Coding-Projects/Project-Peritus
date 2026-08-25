//! Focused canonical-codec and sensitive-value leakage tests.

mod support;

use std::{error::Error, fmt::Write as _};

use peritus_artifact_store::{
    ArtifactDigest, ArtifactMetadata, EncryptionMetadata, FinalizationState, MediaType,
    QuarantineState,
};
use peritus_codec::{CodecLimits, decode_message, encode_message, sha256};
use peritus_trace::{
    Observation, ObservationKind, RedactedValue, SensitivePayload, SensitivityClass, SpanKind,
    TraceErrorKind, redact_sensitive,
};
use peritus_types::Sha256Digest;

use support::{binding, event, observation, span, trace};

const CANARY: &[u8] = b"secret://C7-NEVER-PRINT-9c64";

#[test]
fn sensitive_debug_error_and_complete_source_chain_are_content_free() {
    let payload = SensitivePayload::new(SensitivityClass::Credential, CANARY.to_vec())
        .expect("bounded payload");
    assert!(!format!("{payload:?}").contains("C7-NEVER-PRINT"));

    let metadata = metadata_for(CANARY, u64::try_from(CANARY.len()).expect("canary length") + 1);
    let error = redact_sensitive(payload, Some(&metadata)).expect_err("size mismatch");
    let mut rendered = format!("{error:?}\n{error}");
    let mut source = error.source();
    while let Some(current) = source {
        write!(&mut rendered, "\n{current}").expect("format error chain");
        source = current.source();
    }
    assert!(!rendered.contains("C7-NEVER-PRINT"));
    assert!(error.source().is_none());
    assert_eq!(error.kind(), TraceErrorKind::Redaction);
}

#[test]
fn redaction_omits_bytes_or_binds_exact_encrypted_metadata() {
    let omitted = redact_sensitive(
        SensitivePayload::new(SensitivityClass::Prompt, CANARY.to_vec()).expect("payload"),
        None,
    )
    .expect("omit payload");
    assert!(matches!(
        omitted,
        RedactedValue::Omitted { class: SensitivityClass::Prompt, observed_bytes }
            if observed_bytes == u64::try_from(CANARY.len()).expect("length")
    ));
    assert!(!format!("{omitted:?}").contains("C7-NEVER-PRINT"));

    let metadata = metadata_for(CANARY, u64::try_from(CANARY.len()).expect("length"));
    let vaulted = redact_sensitive(
        SensitivePayload::new(SensitivityClass::Secret, CANARY.to_vec()).expect("payload"),
        Some(&metadata),
    )
    .expect("exact encrypted metadata");
    let reference = vaulted.vault_reference().expect("vault reference");
    assert_eq!(reference.digest(), metadata.digest());
    assert_eq!(reference.size(), metadata.size());

    let plaintext = ArtifactMetadata::new(
        ArtifactDigest::from_sha256(sha256(CANARY)),
        u64::try_from(CANARY.len()).expect("length"),
        MediaType::new("application/octet-stream").expect("media type"),
        EncryptionMetadata::unencrypted(),
        FinalizationState::Finalized,
        event(81),
        QuarantineState::Active,
    );
    let error = redact_sensitive(
        SensitivePayload::new(SensitivityClass::WorkspaceContent, CANARY.to_vec())
            .expect("payload"),
        Some(&plaintext),
    )
    .expect_err("plaintext cannot be referenced");
    assert_eq!(error.kind(), TraceErrorKind::Redaction);
}

#[test]
fn canonical_codec_round_trips_and_rejects_truncation_without_content() {
    let value = observation(
        90,
        trace(91),
        span(92),
        1,
        None,
        Vec::new(),
        binding(9),
        100,
        ObservationKind::SpanStarted(SpanKind::Internal),
    );
    let encoded = encode_message(&value, CodecLimits::PRODUCTION).expect("encode observation");
    let decoded = decode_message::<Observation>(&encoded, CodecLimits::PRODUCTION)
        .expect("decode observation");
    assert_eq!(decoded, value);

    let truncated = &encoded[..encoded.len() - 1];
    let error = decode_message::<Observation>(truncated, CodecLimits::PRODUCTION)
        .expect_err("truncated observation");
    let rendered = format!("{error:?}\n{error}");
    assert!(!rendered.contains("C7-NEVER-PRINT"));
}

fn metadata_for(bytes: &[u8], size: u64) -> ArtifactMetadata {
    ArtifactMetadata::new(
        ArtifactDigest::from_sha256(sha256(bytes)),
        size,
        MediaType::new("application/octet-stream").expect("media type"),
        EncryptionMetadata::envelope(
            "AES-256-GCM",
            Sha256Digest::new([41; 32]),
            Sha256Digest::new([42; 32]),
        )
        .expect("encryption metadata"),
        FinalizationState::Finalized,
        event(80),
        QuarantineState::Active,
    )
}
