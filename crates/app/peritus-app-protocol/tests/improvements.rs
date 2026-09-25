//! Candidate inbox wire compatibility and input validation.
use peritus_app_protocol::{
    AppMessage, AppProtocolLimits, AppRequestEnvelope, AppRequestPayload, AppResponseEnvelope,
    AppResponsePayload, CorrelationId, ImprovementCandidate, ImprovementEvidence, ImprovementInbox,
    ImprovementRequest, ImprovementText, ProductProviderSelection, ProductRunRequest,
    ProtocolContext, ProtocolId, ProtocolVersion, RequestId, WellKnownProtocolFeature,
    decode_app_message, encode_app_message,
};
use peritus_types::{ProviderProfileId, RunId, SessionId, Sha256Digest, WorkspaceId};

fn context() -> ProtocolContext {
    ProtocolContext::new(
        ProtocolId::new([1; 16]).expect("protocol"),
        ProtocolVersion::new(1, 0).expect("version"),
        SessionId::new([2; 16]).expect("session"),
    )
}
fn text(value: &str) -> ImprovementText {
    ImprovementText::new(value.into()).expect("text")
}
fn roundtrip(message: &AppMessage) {
    let encoded = encode_app_message(message, AppProtocolLimits::PRODUCTION).expect("encode");
    assert_eq!(
        &decode_app_message(&encoded, AppProtocolLimits::PRODUCTION).expect("decode"),
        message
    );
}

#[test]
fn all_inbox_operations_round_trip_and_require_explicit_capability() {
    let workspace = WorkspaceId::new([3; 16]).expect("workspace");
    let run = RunId::new([4; 16]).expect("run");
    let candidate = Sha256Digest::new([5; 32]);
    let provider = ProviderProfileId::new([6; 16]).expect("provider");
    for request in [
        ImprovementRequest::List(workspace),
        ImprovementRequest::Suggest { workspace, run, proposal: text("Investigate context loss") },
        ImprovementRequest::Dismiss { workspace, candidate },
        ImprovementRequest::Evaluate {
            workspace,
            candidate,
            run: ProductRunRequest::new(
                run,
                workspace,
                ProductProviderSelection::new(provider, provider, provider),
                "Evaluate".into(),
            )
            .expect("evaluation"),
        },
    ] {
        let payload = AppRequestPayload::Improvements(request);
        assert_eq!(
            payload.required_workbench_feature(),
            Some(WellKnownProtocolFeature::HarnessImprovements)
        );
        roundtrip(&AppMessage::Request(
            AppRequestEnvelope::new(
                context(),
                RequestId::new([7; 16]).expect("request"),
                CorrelationId::new([8; 16]).expect("correlation"),
                payload,
            )
            .expect("envelope"),
        ));
    }
    let evidence =
        ImprovementEvidence::new(run, candidate, text("Observed a missing verification step"));
    let item = ImprovementCandidate::new(
        candidate,
        text("Investigate verification"),
        vec![evidence.clone()],
        Some(run),
        false,
    )
    .expect("candidate");
    let inbox = ImprovementInbox::new(workspace, vec![item]).expect("inbox");
    roundtrip(&AppMessage::Response(AppResponseEnvelope::new(
        context(),
        RequestId::new([7; 16]).expect("request"),
        CorrelationId::new([8; 16]).expect("correlation"),
        AppResponsePayload::Improvements(inbox),
    )));
    assert!(
        ImprovementCandidate::new(
            candidate,
            text("Suggestion"),
            vec![evidence.clone(), evidence],
            None,
            false
        )
        .is_err()
    );
    assert!(ImprovementCandidate::new(candidate, text("Suggestion"), vec![], None, false).is_err());
    assert!(ImprovementText::new("\u{1b}[31m".into()).is_err());
    assert!(ImprovementText::new("x".repeat(4097)).is_err());
}
