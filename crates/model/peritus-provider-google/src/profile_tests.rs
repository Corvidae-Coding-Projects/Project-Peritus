//! Profile validation tests.

use peritus_model_protocol::{Capability, RequestedCapabilities, WireDialect, negotiate};

use super::validate_google_profile;
use crate::test_support::profile;

#[test]
fn accepts_both_explicit_stable_v1_dialects() {
    for dialect in [WireDialect::GeminiInteractionsV1, WireDialect::GeminiGenerateContentV1] {
        validate_google_profile(&profile(dialect)).expect("stable-v1 profile");
    }
}

#[test]
fn unsupported_exact_resume_is_rejected_by_the_profile() {
    let profile = profile(WireDialect::GeminiInteractionsV1);
    let requested =
        RequestedCapabilities::new(&[Capability::ResumableResponse], &[], profile.limits())
            .expect("request");
    assert!(negotiate(&profile, requested).is_err());
}
