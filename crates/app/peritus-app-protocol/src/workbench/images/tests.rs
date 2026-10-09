use super::*;
use crate::{
    AppMessage, AppProtocolLimits, WellKnownProtocolFeature, decode_app_message, encode_app_message,
};
use peritus_types::Sha256Digest;

mod page;

#[test]
fn image_fixtures_roundtrip_every_format_and_independently_gate_upload_preview_and_confirmation() {
    let cases = crate::schema::generated_fixture_cases().expect("fixtures");
    let mut count = 0;
    for case in cases.iter().filter(|case| case.case.contains("workbench-image-")) {
        let message =
            decode_app_message(&case.payload, AppProtocolLimits::PRODUCTION).expect("decode");
        assert_eq!(
            encode_app_message(&message, AppProtocolLimits::PRODUCTION).expect("encode"),
            case.payload
        );
        if let AppMessage::Request(request) = message {
            assert_eq!(
                request.payload().required_workbench_feature(),
                Some(WellKnownProtocolFeature::WorkbenchImages)
            );
        }
        count += 1;
    }
    assert_eq!(count, 11);
}

#[test]
fn image_metadata_and_labels_reject_controls_and_host_limit_violations() {
    for label in ["", "  ", "escape\u{1b}[31m", "new\nline"] {
        assert!(WorkbenchImageLabel::new(label.to_owned()).is_err());
    }
    assert!(WorkbenchImageLabel::new("x".repeat(MAX_WORKBENCH_IMAGE_LABEL_BYTES + 1)).is_err());
    let digest = Sha256Digest::new([1; 32]);
    for (bytes, dimensions, frames) in [
        (0, (1, 1), 1),
        (MAX_WORKBENCH_IMAGE_BYTES + 1, (1, 1), 1),
        (1, (8193, 1), 1),
        (1, (4097, 4096), 1),
        (1, (1, 1), 65),
        (1, (1, 0), 1),
    ] {
        assert!(
            WorkbenchImageMetadata::new(
                digest,
                bytes,
                WorkbenchImageFormat::Png,
                dimensions,
                frames
            )
            .is_err()
        );
    }
    assert!(
        WorkbenchImageMetadata::new(
            digest,
            MAX_WORKBENCH_IMAGE_BYTES,
            WorkbenchImageFormat::Png,
            (4096, 4096),
            64
        )
        .is_ok()
    );
}
