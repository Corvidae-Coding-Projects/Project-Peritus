use crate::{
    AppMessage, AppProtocolLimits, WellKnownProtocolFeature, decode_app_message, encode_app_message,
};

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
                Some(WellKnownProtocolFeature::WorkbenchFiles)
            );
        }
        count += 1;
    }
    assert_eq!(count, 14);
}
