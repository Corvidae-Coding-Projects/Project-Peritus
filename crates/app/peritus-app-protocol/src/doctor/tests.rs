use super::*;

fn finding() -> DoctorFinding {
    DoctorFinding::new(
        "local".to_owned(),
        DoctorStatus::Warning,
        "No inference was run.".to_owned(),
        String::new(),
    )
    .expect("finding")
}

#[test]
fn rejects_controls_oversized_text_duplicates_and_empty_reports() {
    for text in [
        String::new(),
        "x".repeat(MAX_DOCTOR_TEXT_BYTES + 1),
        "\u{1b}[2J".to_owned(),
        "\n".to_owned(),
    ] {
        assert!(
            DoctorFinding::new("local".to_owned(), DoctorStatus::Healthy, text, String::new())
                .is_err()
        );
    }
    let query = DoctorQuery::new(WorkspaceId::new([1; 16]).expect("workspace"), None);
    assert!(DoctorReport::new(query, Vec::new()).is_err());
    assert!(DoctorReport::new(query, vec![finding(), finding()]).is_err());
    assert!(DoctorReport::new(query, vec![finding(); MAX_DOCTOR_FINDINGS + 1]).is_err());
}

#[test]
fn report_and_optional_provider_roundtrip_through_public_wire() {
    use crate::{
        AppMessage, AppProtocolLimits, AppResponseEnvelope, AppResponsePayload, CorrelationId,
        ProtocolContext, ProtocolId, ProtocolVersion, RequestId, decode_app_message,
        encode_app_message,
    };
    for provider in [None, Some(ProviderProfileId::new([7; 16]).expect("profile"))] {
        let query = DoctorQuery::new(WorkspaceId::new([1; 16]).expect("workspace"), provider);
        let report = DoctorReport::new(query, vec![finding()]).expect("report");
        let message = AppMessage::Response(AppResponseEnvelope::new(
            ProtocolContext::new(
                ProtocolId::new([2; 16]).expect("protocol"),
                ProtocolVersion::new(1, 0).expect("version"),
                peritus_types::SessionId::new([3; 16]).expect("session"),
            ),
            RequestId::new([4; 16]).expect("request"),
            CorrelationId::new([5; 16]).expect("correlation"),
            AppResponsePayload::Doctor(report),
        ));
        let bytes = encode_app_message(&message, AppProtocolLimits::PRODUCTION).expect("encode");
        assert_eq!(
            decode_app_message(&bytes, AppProtocolLimits::PRODUCTION).expect("decode"),
            message
        );
    }
}
