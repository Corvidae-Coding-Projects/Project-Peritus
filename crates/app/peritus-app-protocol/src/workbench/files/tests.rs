use crate::{
    AppMessage, AppProtocolLimits, WellKnownProtocolFeature, decode_app_message, encode_app_message,
};

#[test]
fn extended_paths_and_native_labels_roundtrip_through_preview_and_file_page_frames() {
    let cases = crate::schema::generated_fixture_cases().unwrap();
    let case = cases.iter().find(|case| case.case == "realistic-workbench-file-preview").unwrap();
    let AppMessage::Response(response) =
        decode_app_message(&case.payload, AppProtocolLimits::PRODUCTION).unwrap()
    else {
        panic!("response")
    };
    let crate::AppResponsePayload::WorkbenchFilePreview(original) = response.payload() else {
        panic!("preview")
    };
    let extended = vec!["directory-component"; 257].join("/");
    assert!(extended.len() > 4_096);
    for path in [extended.as_str(), "native:reference.\n", "escape\u{1b}"] {
        let request = crate::WorkbenchFileRequest::new(
            original.request().query(),
            original.request().revision(),
            path.to_owned(),
            original.request().range(),
            original.request().mode(),
            original.request().provider(),
            original.request().model().clone(),
        )
        .unwrap();
        let preview = crate::WorkbenchFilePreview::new(
            request,
            original.folder(),
            original.file(),
            original.provider_revision(),
            original.resolved_model().to_owned(),
        )
        .unwrap();
        let message = AppMessage::Response(crate::AppResponseEnvelope::new(
            response.context(),
            response.request_id(),
            response.correlation_id(),
            crate::AppResponsePayload::WorkbenchFilePreview(preview),
        ));
        let bytes = encode_app_message(&message, AppProtocolLimits::PRODUCTION).unwrap();
        assert_eq!(decode_app_message(&bytes, AppProtocolLimits::PRODUCTION).unwrap(), message);
        let row = crate::WorkbenchFileRow::new(
            crate::ControlOperationId::new([3; 16]).unwrap(),
            crate::ControlOperationId::new([4; 16]).unwrap(),
            path.to_owned(),
            original.request().mode(),
            original.file(),
            true,
            true,
        )
        .unwrap();
        let query = crate::WorkbenchFileQuery::new(
            original.request().query(),
            original.request().revision(),
            0,
        )
        .unwrap();
        let page = crate::WorkbenchFilePage::new(query, vec![row], 1).unwrap();
        let message = AppMessage::Response(crate::AppResponseEnvelope::new(
            response.context(),
            response.request_id(),
            response.correlation_id(),
            crate::AppResponsePayload::WorkbenchFiles(page),
        ));
        let bytes = encode_app_message(&message, AppProtocolLimits::PRODUCTION).unwrap();
        assert_eq!(decode_app_message(&bytes, AppProtocolLimits::PRODUCTION).unwrap(), message);
        assert_eq!(
            encode_app_message(
                &decode_app_message(&bytes, AppProtocolLimits::PRODUCTION).unwrap(),
                AppProtocolLimits::PRODUCTION
            )
            .unwrap(),
            bytes
        );
    }
    assert!(
        crate::WorkbenchFileRequest::new(
            original.request().query(),
            original.request().revision(),
            "bad\0path".to_owned(),
            original.request().range(),
            original.request().mode(),
            original.request().provider(),
            original.request().model().clone()
        )
        .is_err()
    );
}

#[test]
fn large_source_preview_and_ranges_keep_existing_wire_representation_and_integrity_checks() {
    use crate::{
        ProductModelChoice, WorkbenchFileMetadata, WorkbenchFileMode, WorkbenchFilePreview,
        WorkbenchFileRange, WorkbenchFileRequest,
    };
    use peritus_types::{ProviderProfileId, Sha256Digest};
    let cases = crate::schema::generated_fixture_cases().unwrap();
    let case = cases.iter().find(|case| case.case == "realistic-workbench-file-preview").unwrap();
    let AppMessage::Response(response) =
        decode_app_message(&case.payload, AppProtocolLimits::PRODUCTION).unwrap()
    else {
        panic!("response")
    };
    let crate::AppResponsePayload::WorkbenchFilePreview(original) = response.payload() else {
        panic!("preview")
    };
    let start = 64 * 1024 * 1024;
    let source = u64::MAX;
    let range = WorkbenchFileRange::Bytes { start, end: start + 1 };
    let request = WorkbenchFileRequest::new(
        original.request().query(),
        original.request().revision(),
        original.request().path().to_owned(),
        range,
        WorkbenchFileMode::Snapshot,
        ProviderProfileId::new([3; 16]).unwrap(),
        ProductModelChoice::default(),
    )
    .unwrap();
    let metadata = WorkbenchFileMetadata::new(
        Sha256Digest::new([4; 32]),
        source,
        (start, start + 1),
        Sha256Digest::new([5; 32]),
    )
    .unwrap();
    let preview =
        WorkbenchFilePreview::new(request, original.folder(), metadata, 1, "model".to_owned())
            .unwrap();
    let envelope = crate::AppResponseEnvelope::new(
        response.context(),
        response.request_id(),
        response.correlation_id(),
        crate::AppResponsePayload::WorkbenchFilePreview(preview),
    );
    let envelope = AppMessage::Response(envelope);
    let bytes = encode_app_message(&envelope, AppProtocolLimits::PRODUCTION).unwrap();
    let decoded = decode_app_message(&bytes, AppProtocolLimits::PRODUCTION).unwrap();
    assert_eq!(decoded, envelope);
    assert_eq!(encode_app_message(&decoded, AppProtocolLimits::PRODUCTION).unwrap(), bytes);
    assert!(WorkbenchFileRange::Lines { first: 67_108_865, last: u32::MAX }.validate().is_ok());
    assert!(WorkbenchFileRange::Bytes { start: 2, end: 1 }.validate().is_err());
    assert!(WorkbenchFileRange::Lines { first: 0, last: 1 }.validate().is_err());
    assert!(
        WorkbenchFileMetadata::new(metadata.source_digest(), source, (2, 1), metadata.digest())
            .is_err()
    );
    assert!(
        WorkbenchFileMetadata::new(metadata.source_digest(), 1, (1, 2), metadata.digest()).is_err()
    );
    assert!(
        WorkbenchFileMetadata::new(
            metadata.source_digest(),
            source,
            (0, crate::MAX_WORKBENCH_FILE_BYTES + 1),
            metadata.digest()
        )
        .is_ok()
    );
}

#[test]
fn file_compatibility_frames_roundtrip_and_require_independent_feature() {
    let cases = crate::schema::generated_fixture_cases().expect("fixtures");
    let mut count = 0;
    for case in cases.iter().filter(|case| case.case.contains("workbench-file-")) {
        let message =
            decode_app_message(&case.payload, AppProtocolLimits::PRODUCTION).expect("decode");
        assert_eq!(
            encode_app_message(&message, AppProtocolLimits::PRODUCTION).expect("encode"),
            case.payload
        );
        if let AppMessage::Request(request) = message {
            assert_eq!(
                request.payload().required_workbench_feature(),
                Some(if case.case.contains("workbench-file-source-") {
                    WellKnownProtocolFeature::WorkbenchFileSources
                } else {
                    WellKnownProtocolFeature::WorkbenchFiles
                })
            );
        }
        count += 1;
    }
    assert_eq!(count, 16);
}
