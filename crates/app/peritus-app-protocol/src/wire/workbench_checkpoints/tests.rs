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

fn response_round_trip(payload: crate::AppResponsePayload) {
    let context = ProtocolContext::new(
        ProtocolId::new([5; 16]).unwrap(),
        ProtocolVersion::new(1, 0).unwrap(),
        SessionId::new([6; 16]).unwrap(),
    );
    let message = AppMessage::Response(crate::AppResponseEnvelope::new(
        context,
        RequestId::new([7; 16]).unwrap(),
        CorrelationId::new([8; 16]).unwrap(),
        payload,
    ));
    let bytes = encode_app_message(&message, AppProtocolLimits::PRODUCTION)
        .expect("complete public checkpoint message");
    assert_eq!(decode_app_message(&bytes, AppProtocolLimits::PRODUCTION).unwrap(), message);
}

#[test]
fn public_manifest_name_path_and_derived_text_have_no_field_allowance() {
    let query = preview().request().query();
    let path = std::iter::repeat_n("component", 470).collect::<Vec<_>>().join("/");
    let checkpoint = WorkbenchCheckpointReceipt::new(
        ControlOperationId::new([3; 16]).unwrap(),
        query,
        4,
        WorkbenchCheckpointName::new("é".repeat(129)).expect("258-byte name"),
        WorkbenchCheckpointReferences::new(1, 0, 0, None),
        vec![
            WorkbenchCheckpointPath::new(path, WorkbenchCheckpointVersion::Absent, None)
                .expect("native-sized path"),
        ],
        vec![format!("{}: unselected source", "x".repeat(600))],
        vec!["effect".repeat(110)],
    )
    .expect("complete metadata");
    response_round_trip(crate::AppResponsePayload::WorkbenchCheckpoint(checkpoint));
}

#[test]
fn public_manifest_and_preview_cover_more_than_65535_paths_and_explanations() {
    let baseline = preview();
    let paths = (0..=u16::MAX)
        .map(|index| {
            WorkbenchRewindPath::new(
                format!("file-{index:05}"),
                WorkbenchCheckpointVersion::Absent,
                None,
                WorkbenchCheckpointVersion::Absent,
                WorkbenchRewindDisposition::Unchanged,
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let exclusions = (0..=u16::MAX).map(|_| "excluded".to_owned()).collect::<Vec<_>>();
    let external = (0..=u16::MAX).map(|_| "external".to_owned()).collect::<Vec<_>>();
    let checkpoint = WorkbenchCheckpointReceipt::new(
        ControlOperationId::new([3; 16]).unwrap(),
        baseline.request().query(),
        4,
        WorkbenchCheckpointName::new("complete".to_owned()).unwrap(),
        WorkbenchCheckpointReferences::new(1, 0, 0, None),
        paths
            .iter()
            .map(|path| {
                WorkbenchCheckpointPath::new(path.path().to_owned(), path.checkpoint(), None)
                    .unwrap()
            })
            .collect(),
        exclusions.clone(),
        external.clone(),
    )
    .expect("65536 public coverage entries");
    response_round_trip(crate::AppResponsePayload::WorkbenchCheckpoint(checkpoint));
    let preview = WorkbenchRewindPreview::new(baseline.request(), paths, exclusions, external)
        .expect("65536 confirmation facts");
    response_round_trip(crate::AppResponsePayload::WorkbenchRewindPreview(preview));
}

#[test]
fn public_restore_conflict_receipt_retains_wide_counts_and_long_paths() {
    let conflicts = (0..=u16::MAX).map(|index| format!("file-{index:05}")).collect();
    let receipt = WorkbenchRestoreReceipt::new(
        ControlOperationId::new([9; 16]).unwrap(),
        ControlOperationId::new([3; 16]).unwrap(),
        ControlOperationId::new([10; 16]).unwrap(),
        preview().request().query(),
        5,
        WorkbenchRestoreStatus::Conflict,
        Vec::new(),
        conflicts,
        vec!["long metadata".repeat(400)],
    )
    .expect("65536 exact conflicts");
    response_round_trip(crate::AppResponsePayload::WorkbenchRestore(receipt));
}

#[test]
fn wide_counts_reject_unavailable_encoded_entries_before_allocation() {
    for count in [2, u64::MAX] {
        let mut bytes = count.to_be_bytes().to_vec();
        bytes.extend_from_slice(&[0, 0, 0, 1, b'x']);
        let mut reader = CanonicalReader::new(&bytes, CodecLimits::PRODUCTION);
        let error = read_strings(&mut reader, ManifestForm::Wide).unwrap_err();
        assert!(matches!(error.kind(), CodecErrorKind::Truncated | CodecErrorKind::LengthOverflow));
    }
}

#[test]
fn manifest_forms_are_canonical_and_wide_confirmation_rejects_tampering() {
    let baseline = preview();
    let wide = WorkbenchRewindPreview::new(
        baseline.request(),
        baseline.paths().to_vec(),
        vec!["explanation".repeat(60)],
        Vec::new(),
    )
    .unwrap();
    for (value, form, accepted) in [
        (&baseline, ManifestForm::Wide, false),
        (&wide, ManifestForm::Legacy, false),
        (&wide, ManifestForm::Wide, true),
    ] {
        let mut writer = CanonicalWriter::new(CodecLimits::PRODUCTION);
        write_request(&mut writer, value.request()).unwrap();
        write_digest(&mut writer, value.preview_digest()).unwrap();
        write_rewind_paths(&mut writer, value.paths(), form).unwrap();
        write_strings(&mut writer, value.exclusions(), form).unwrap();
        write_strings(&mut writer, value.external_effects(), form).unwrap();
        writer.write_bool(true).unwrap();
        writer.write_bool(true).unwrap();
        let mut bytes = writer.into_bytes();
        let mut reader = CanonicalReader::new(&bytes, CodecLimits::PRODUCTION);
        if accepted {
            assert_eq!(read_preview_as(&mut reader, true).unwrap(), wide);
            reader.finish().unwrap();
            let digest = wide.preview_digest();
            let offset = bytes.windows(32).position(|bytes| bytes == digest.as_bytes()).unwrap();
            bytes[offset] ^= 1;
            let mut reader = CanonicalReader::new(&bytes, CodecLimits::PRODUCTION);
            assert_eq!(
                read_preview_as(&mut reader, true).unwrap_err().kind(),
                CodecErrorKind::InvalidDomainValue
            );
        } else {
            assert_eq!(
                read_preview_as(&mut reader, form.is_wide()).unwrap_err().kind(),
                CodecErrorKind::InvalidDomainValue
            );
        }
    }
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
