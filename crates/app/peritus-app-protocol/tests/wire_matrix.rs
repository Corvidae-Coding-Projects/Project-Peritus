//! Six-family canonical wire round-trip and rejection integration tests.

use peritus_app_protocol::{
    AppErrorCode, AppProtocolLimits, decode_app_message, encode_app_message,
    schema::generated_fixture_cases,
};
use std::collections::BTreeSet;

#[test]
fn all_six_families_round_trip_and_reject_malformed_frames() {
    let fixtures = generated_fixture_cases().expect("canonical fixtures encode");
    let mut observed_families = BTreeSet::new();
    for fixture in &fixtures {
        let decoded = decode_app_message(&fixture.payload, AppProtocolLimits::PRODUCTION);
        if fixture.accepted {
            let message = decoded.expect("valid fixture decodes");
            observed_families.insert(message.family());
            assert_eq!(
                encode_app_message(&message, AppProtocolLimits::PRODUCTION)
                    .expect("decoded message re-encodes"),
                fixture.payload,
                "{} is not byte-canonical",
                fixture.case,
            );
        } else {
            assert_eq!(
                decoded.expect_err("invalid fixture rejects").code(),
                fixture.expected_error.expect("invalid fixture names stable error"),
                "{} returned the wrong stable error",
                fixture.case,
            );
        }
    }
    assert_eq!(observed_families, BTreeSet::from([94, 95, 96, 97, 98, 99]));

    let valid = fixtures
        .iter()
        .find(|fixture| fixture.case == "minimal-client-hello")
        .expect("minimal hello fixture");
    let mut truncated = valid.payload.clone();
    truncated.pop();
    assert_eq!(
        decode_app_message(&truncated, AppProtocolLimits::PRODUCTION)
            .expect_err("truncated payload rejects")
            .code(),
        AppErrorCode::TruncatedFrame,
    );

    let mut trailing = valid.payload.clone();
    trailing.push(0);
    assert_eq!(
        decode_app_message(&trailing, AppProtocolLimits::PRODUCTION)
            .expect_err("trailing bytes reject")
            .code(),
        AppErrorCode::TrailingBytes,
    );
}
