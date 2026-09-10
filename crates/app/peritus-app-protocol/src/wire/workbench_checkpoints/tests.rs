use super::*;
use crate::{
    AppMessage, AppProtocolLimits, AppRequestEnvelope, AppRequestPayload, ConversationId,
    CorrelationId, ProtocolContext, ProtocolId, ProtocolVersion, RequestId,
    WorkbenchCheckpointVersion, WorkbenchQuery, decode_app_message, encode_app_message,
};
use peritus_codec::{CanonicalEncode, CodecLimits};
use peritus_types::SessionId;

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
fn rewind_modes_child_and_allocation_are_in_the_exact_preview_fingerprint() {
    let baseline = preview();
    let child = ConversationId::new([19; 16]).unwrap();
    let budget = crate::WorkbenchForkBudget::new(1000, 2, 3, 400).unwrap();
    for mode in [WorkbenchRewindMode::ConversationOnly, WorkbenchRewindMode::Combined] {
        for allocation in [None, Some(budget)] {
            let request = baseline.request().with_branch(mode, child, allocation).unwrap();
            let paths = if mode == WorkbenchRewindMode::ConversationOnly {
                Vec::new()
            } else {
                baseline.paths().to_vec()
            };
            let expected =
                WorkbenchRewindPreview::new(request, paths, Vec::new(), Vec::new()).unwrap();
            assert_ne!(expected.preview_digest(), baseline.preview_digest());
            let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
            write_preview(&mut writer, &expected).unwrap();
            let bytes = writer.into_bytes();
            let mut reader = CanonicalReader::new(&bytes, CodecLimits::PRODUCTION);
            assert_eq!(read_preview(&mut reader).unwrap(), expected);
        }
    }
    assert!(baseline.request().with_branch(WorkbenchRewindMode::FilesOnly, child, None).is_err());
    assert!(
        baseline
            .request()
            .with_branch(
                WorkbenchRewindMode::Combined,
                baseline.request().query().conversation(),
                None
            )
            .is_err()
    );
    let request =
        baseline.request().with_branch(WorkbenchRewindMode::ConversationOnly, child, None).unwrap();
    assert!(
        WorkbenchRewindPreview::new(request, baseline.paths().to_vec(), Vec::new(), Vec::new())
            .is_err()
    );
}
