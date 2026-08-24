//! Error normalization tests.

use peritus_model_protocol::{FailureCategory, WireDialect};

use super::status_failure;
use crate::test_support::profile;

#[test]
fn rate_quota_auth_and_transient_statuses_remain_distinct() {
    let provider = profile(WireDialect::GeminiInteractionsV1).provider().clone();
    let cases = [
        (401, false, FailureCategory::Authentication),
        (429, false, FailureCategory::RateLimited),
        (429, true, FailureCategory::QuotaExhausted),
        (503, false, FailureCategory::TransientProvider),
    ];
    for (status, quota, expected) in cases {
        let failure =
            status_failure(provider.clone(), status, Some(250), quota, None).expect("failure");
        assert_eq!(failure.category(), expected);
        assert_eq!(failure.http_status(), Some(status));
        assert!(!format!("{failure:?}").contains("upstream secret"));
    }
}
