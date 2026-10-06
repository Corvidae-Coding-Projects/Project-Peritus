use super::*;
use crate::WorkbenchCheckpointFileMode as FileMode;
use crate::{
    AppMessage, AppProtocolLimits, AppRequestEnvelope, AppRequestPayload, ConversationId,
    CorrelationId, ProtocolContext, ProtocolId, ProtocolVersion, RequestId,
    WorkbenchCheckpointVersion, WorkbenchQuery, decode_app_message, encode_app_message,
};
use peritus_codec::{CanonicalEncode, CodecLimits};
use peritus_types::SessionId;

#[test]
fn directory_and_selected_range_encodings_round_trip_and_bind_scope_into_confirmation() {
    use crate::{WorkbenchCheckpointRange, WorkbenchFileRange};
    let baseline = preview();
    let directory = WorkbenchCheckpointVersion::EmptyDirectory { permissions: 0o750 };
    let source = WorkbenchCheckpointVersion::Present {
        digest: Sha256Digest::new([4; 32]),
        bytes: 20,
        mode: FileMode::Regular,
    };
    let current = WorkbenchCheckpointVersion::Present {
        digest: Sha256Digest::new([5; 32]),
        bytes: 30,
        mode: FileMode::Executable,
    };
    let selected =
        WorkbenchCheckpointRange::new(WorkbenchFileRange::Lines { first: 2, last: 2 }, 4, 10)
            .unwrap();
    let make = |ranges| {
        WorkbenchRewindPreview::new(
            baseline.request(),
            vec![
                WorkbenchRewindPath::new(
                    "empty".to_owned(),
                    directory,
                    Some(WorkbenchCheckpointVersion::Absent),
                    WorkbenchCheckpointVersion::Absent,
                    WorkbenchRewindDisposition::Restore,
                )
                .unwrap(),
                WorkbenchRewindPath::new_with_ranges(
                    "note.txt".to_owned(),
                    source,
                    Some(current),
                    current,
                    WorkbenchRewindDisposition::Restore,
                    ranges,
                )
                .unwrap(),
            ],
            Vec::new(),
            Vec::new(),
        )
        .unwrap()
    };
    let whole = make(Vec::new());
    let ranged = make(vec![selected]);
    assert_ne!(whole.preview_digest(), ranged.preview_digest());
    let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
    write_preview(&mut writer, &ranged).unwrap();
    let encoded = writer.into_bytes();
    let mut reader = CanonicalReader::new(&encoded, CodecLimits::PRODUCTION);
    assert_eq!(read_preview(&mut reader).unwrap(), ranged);
    reader.finish().unwrap();
    let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
    write_version(&mut writer, directory).unwrap();
    assert_eq!(writer.into_bytes(), [0, 2, 1, 232]);
}

#[test]
fn old_whole_file_encoding_is_retained_exactly_and_empty_range_envelopes_are_rejected() {
    let version = WorkbenchCheckpointVersion::Present {
        digest: Sha256Digest::new([4; 32]),
        bytes: 5,
        mode: FileMode::Regular,
    };
    let mut legacy = vec![0, 1];
    legacy.extend_from_slice(&[4; 32]);
    legacy.extend_from_slice(&5_u64.to_be_bytes());
    legacy.extend_from_slice(&[0, 1]);
    let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
    write_captured(&mut writer, version, &[]).unwrap();
    assert_eq!(writer.into_bytes(), legacy);
    let mut malformed = vec![0, 3];
    malformed.extend_from_slice(&legacy);
    malformed.extend_from_slice(&0_u64.to_be_bytes());
    assert!(read_captured(&mut CanonicalReader::new(&malformed, CodecLimits::PRODUCTION)).is_err());
    malformed.truncate(malformed.len() - 8);
    malformed.extend_from_slice(&u64::MAX.to_be_bytes());
    assert!(read_captured(&mut CanonicalReader::new(&malformed, CodecLimits::PRODUCTION)).is_err());
}

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
