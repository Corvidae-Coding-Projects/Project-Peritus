use super::*;
use crate::{
    AppMessage, AppProtocolLimits, AppRequestEnvelope, AppRequestPayload, ConversationId,
    CorrelationId, ProtocolContext, ProtocolId, ProtocolVersion, RequestId,
    WorkbenchCheckpointVersion, WorkbenchQuery, decode_app_message, encode_app_message,
};
use peritus_codec::{CanonicalEncode, CodecLimit, CodecLimits};
use peritus_types::{SessionId, Sha256Digest};

fn preview() -> WorkbenchRewindPreview {
    let query = WorkbenchQuery::new(
        ConversationId::new([1; 16]).expect("conversation"),
        peritus_types::WorkspaceId::new([2; 16]).expect("workspace"),
    );
    let request = WorkbenchRewindRequest::new(
        query,
        4,
        ControlOperationId::new([3; 16]).expect("checkpoint"),
    )
    .expect("request");
    WorkbenchRewindPreview::new(
        request,
        vec![
            WorkbenchRewindPath::new(
                "note.txt".to_owned(),
                WorkbenchCheckpointVersion::Absent,
                Some(WorkbenchCheckpointVersion::Present {
                    digest: Sha256Digest::new([4; 32]),
                    bytes: 5,
                    mode: FileMode::Regular,
                }),
                WorkbenchCheckpointVersion::Present {
                    digest: Sha256Digest::new([4; 32]),
                    bytes: 5,
                    mode: FileMode::Regular,
                },
                WorkbenchRewindDisposition::Restore,
            )
            .expect("path"),
        ],
        vec!["partial.txt: partial selection".to_owned()],
        vec!["external effects excluded".to_owned()],
    )
    .expect("preview")
}

#[test]
fn preview_round_trips_and_rejects_a_tampered_fingerprint() {
    let expected = preview();
    let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
    write_preview(&mut writer, &expected).expect("encode");
    let bytes = writer.into_bytes();
    let mut reader = CanonicalReader::new(&bytes, CodecLimits::PRODUCTION);
    assert_eq!(read_preview(&mut reader).expect("decode"), expected);

    let fingerprint = expected.preview_digest();
    let digest = fingerprint.as_bytes();
    let offset =
        bytes.windows(digest.len()).position(|window| window == digest).expect("fingerprint bytes");
    let mut tampered = bytes;
    tampered[offset] ^= 1;
    let mut reader = CanonicalReader::new(&tampered, CodecLimits::PRODUCTION);
    assert_eq!(
        read_preview(&mut reader).expect_err("tampered digest").kind(),
        CodecErrorKind::InvalidDomainValue
    );
}

#[test]
fn small_preview_fingerprint_keeps_the_legacy_canonical_bytes() {
    let expected = preview();
    let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
    writer.write_fixed(b"peritus-workbench-rewind-preview-v1").expect("domain tag");
    write_request(&mut writer, expected.request()).expect("request");
    write_rewind_paths(&mut writer, expected.paths()).expect("paths");
    write_strings(&mut writer, expected.exclusions()).expect("exclusions");
    write_strings(&mut writer, expected.external_effects()).expect("external effects");

    assert_eq!(expected.preview_digest(), peritus_codec::sha256(writer.as_slice()));
}

#[test]
fn checkpoint_inspection_request_uses_tag_121_and_round_trips() {
    let request = preview().request();
    let context = ProtocolContext::new(
        ProtocolId::new([5; 16]).expect("protocol"),
        ProtocolVersion::new(1, 0).expect("version"),
        SessionId::new([6; 16]).expect("session"),
    );
    let envelope = AppRequestEnvelope::new(
        context,
        RequestId::new([7; 16]).expect("request"),
        CorrelationId::new([8; 16]).expect("correlation"),
        AppRequestPayload::InspectWorkbenchCheckpoint(request),
    )
    .expect("envelope");

    let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
    envelope.encode_payload(&mut writer).expect("payload");
    let bytes = writer.into_bytes();
    let mut reader = CanonicalReader::new(&bytes, CodecLimits::PRODUCTION);
    super::super::primitive::read_context(&mut reader).expect("context");
    reader.read_fixed::<16>().expect("request id");
    reader.read_fixed::<16>().expect("correlation id");
    assert_eq!(reader.read_u16().expect("payload tag"), 121);

    let message = AppMessage::Request(envelope);
    let frame = encode_app_message(&message, AppProtocolLimits::PRODUCTION).expect("frame");
    assert_eq!(decode_app_message(&frame, AppProtocolLimits::PRODUCTION).expect("decode"), message,);
}

#[test]
fn rewind_modes_and_child_are_in_the_exact_preview_fingerprint() {
    let baseline = preview();
    let child = ConversationId::new([19; 16]).unwrap();
    for mode in [WorkbenchRewindMode::ConversationOnly, WorkbenchRewindMode::Combined] {
        let request = baseline.request().with_branch(mode, child).unwrap();
        let paths = if mode == WorkbenchRewindMode::ConversationOnly {
            Vec::new()
        } else {
            baseline.paths().to_vec()
        };
        let expected = WorkbenchRewindPreview::new(request, paths, Vec::new(), Vec::new()).unwrap();
        assert_ne!(expected.preview_digest(), baseline.preview_digest());
        let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
        write_preview(&mut writer, &expected).unwrap();
        let bytes = writer.into_bytes();
        let mut reader = CanonicalReader::new(&bytes, CodecLimits::PRODUCTION);
        assert_eq!(read_preview(&mut reader).unwrap(), expected);
    }
    assert!(baseline.request().with_branch(WorkbenchRewindMode::FilesOnly, child).is_err());
    assert!(
        baseline
            .request()
            .with_branch(WorkbenchRewindMode::Combined, baseline.request().query().conversation())
            .is_err()
    );
    let request =
        baseline.request().with_branch(WorkbenchRewindMode::ConversationOnly, child).unwrap();
    assert!(
        WorkbenchRewindPreview::new(request, baseline.paths().to_vec(), Vec::new(), Vec::new())
            .is_err()
    );
}

#[test]
fn preview_fingerprint_streams_aggregate_payloads_beyond_legacy_limit() {
    let request = preview().request();
    let exclusions = vec!["x".repeat(4_610); 4_000];
    assert!(exclusions.len() < usize::from(u16::MAX));
    assert!(
        exclusions.len() * (4_610 + 4) > CodecLimits::PRODUCTION.max_payload_bytes,
        "the legacy combined encoding exceeds its aggregate payload limit"
    );

    let oversized = WorkbenchRewindPreview::new(request, Vec::new(), exclusions, Vec::new())
        .expect("valid facts remain fingerprintable beyond the legacy aggregate payload cap");

    assert_ne!(oversized.preview_digest(), Sha256Digest::new([0; 32]),);
}

#[test]
fn streaming_preview_fingerprint_preserves_individual_string_limits() {
    let request = preview().request();
    let exclusions = vec!["x".repeat(CodecLimits::PRODUCTION.max_string_bytes + 1)];

    let error = preview_fingerprint(request, &[], &exclusions, &[])
        .expect_err("an individually oversized fact remains invalid");

    assert_eq!(error.limit(), Some(CodecLimit::StringBytes));
}
