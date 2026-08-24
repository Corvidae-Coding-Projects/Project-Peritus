//! Redacted HTTP and ambiguity classification tables.

use peritus_model_protocol::{
    FailureCategory, OutcomeCertainty, ProviderName, ResponseId, Retryability, TransportPhase,
};

use super::{ambiguous_transport, status_failure};

fn provider() -> ProviderName {
    ProviderName::new("anthropic".to_owned()).expect("provider")
}

#[test]
fn official_http_statuses_map_to_stable_typed_failures() {
    let cases = [
        (400, false, FailureCategory::InvalidRequest, Retryability::Never),
        (401, false, FailureCategory::Authentication, Retryability::Never),
        (402, false, FailureCategory::QuotaExhausted, Retryability::Never),
        (403, false, FailureCategory::Permission, Retryability::Never),
        (404, false, FailureCategory::NotFound, Retryability::Never),
        (409, false, FailureCategory::TransientProvider, Retryability::SafeNewRequest),
        (413, false, FailureCategory::InvalidRequest, Retryability::Never),
        (429, false, FailureCategory::RateLimited, Retryability::SafeNewRequest),
        (429, true, FailureCategory::QuotaExhausted, Retryability::Never),
        (500, false, FailureCategory::TransientProvider, Retryability::SafeNewRequest),
        (504, false, FailureCategory::TransientProvider, Retryability::SafeNewRequest),
        (529, false, FailureCategory::TransientProvider, Retryability::SafeNewRequest),
    ];
    for (status, quota, category, retryability) in cases {
        let response_id = ResponseId::new("req_failure".to_owned()).expect("response ID");
        let failure = status_failure(provider(), status, Some(250), quota, Some(response_id))
            .expect("failure");
        assert_eq!(failure.category(), category);
        assert_eq!(failure.retryability(), retryability);
        assert_eq!(failure.http_status(), Some(status));
        assert_eq!(failure.retry_after_millis(), Some(250));
        assert_eq!(failure.certainty(), OutcomeCertainty::DefinitelyNotAccepted);
        assert_eq!(failure.phase(), TransportPhase::ReadingBody);
        assert_eq!(failure.response_id().map(ResponseId::expose_for_wire), Some("req_failure"));
    }
}

#[test]
fn maybe_sent_transport_is_ambiguous_and_diagnostics_are_redacted() {
    let failure = ambiguous_transport(provider()).expect("failure");
    assert_eq!(failure.category(), FailureCategory::AmbiguousAcceptance);
    assert_eq!(failure.certainty(), OutcomeCertainty::MaybeAccepted);
    assert_eq!(failure.retryability(), Retryability::CallerDecision);
    assert_eq!(failure.phase(), TransportPhase::SendingBody);
    assert!(!format!("{failure:?}").contains("secret-provider-body"));
}
