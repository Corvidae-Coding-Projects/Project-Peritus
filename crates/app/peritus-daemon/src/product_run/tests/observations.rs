//! A running task beside an unqualified candidate must not disconnect any client.
use super::*;
use crate::product_run::ProductRunServiceError;
use peritus_app_protocol::AppResponsePayload;

#[test]
fn mixed_active_and_candidate_queries_encode_with_exact_evidence() {
    interaction::block_on(async {
        let repository = repository();
        let state = tempfile::tempdir().expect("state");
        let writer = support::stalled_after(0xb1, "writer", complete_writer(CORRECT));
        let reviewer = scripted(0xb2, "reviewer", Vec::new());
        let fixer = scripted(0xb3, "fixer", Vec::new());
        let workspace = WorkspaceId::new([0xb4; 16]).expect("workspace");
        let service =
            service(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
        let candidate = RunId::new([0xb5; 16]).expect("candidate");
        let active = RunId::new([0xb6; 16]).expect("active");
        let providers = ProductProviderSelection::new(
            writer.profile.profile_id(),
            reviewer.profile.profile_id(),
            fixer.profile.profile_id(),
        );
        service
            .start(
                ProductRunRequest::new(
                    candidate,
                    workspace,
                    providers,
                    "Add a tested answer function that returns 42.".to_owned(),
                )
                .expect("request"),
            )
            .await
            .expect("start candidate");
        let failed = wait_for_terminal(&service, candidate).await;
        assert_eq!(failed.phase(), ProductRunPhase::Failed);
        assert!(failed.deliverable().is_some());
        let binding = service.query_interaction_binding(
            peritus_types::ActorId::new([1; 16]).unwrap(),
            peritus_app_protocol::ProductRunConversationQuery::new(candidate),
        );
        assert!(
            matches!(binding, Err(ProductRunServiceError::InvalidState)),
            "noninteractive coding runs keep their explicit follow-up editor fallback"
        );
        service
            .start(
                ProductRunRequest::new(
                    active,
                    workspace,
                    providers,
                    "Explain the code.".to_owned(),
                )
                .expect("request"),
            )
            .await
            .expect("start active");
        let values = service.query_observations(ProductRunQuery::recent()).expect("observations");
        assert_eq!(values.len(), 2);
        let running = values.iter().find(|v| v.snapshot().run_id() == active).expect("running");
        assert!(running.settlement().is_none());
        let retained =
            values.iter().find(|v| v.snapshot().run_id() == candidate).expect("candidate");
        assert_eq!(retained.snapshot().deliverable(), failed.deliverable());
        assert_eq!(
            retained.settlement().expect("settlement").disposition(),
            peritus_run_settlement::RunDisposition::CandidateAvailable
        );
        encode(AppResponsePayload::ProductRunObservations(values));
        let observations = service.query_observations(ProductRunQuery::recent()).expect("mixed");
        assert!(
            observations
                .iter()
                .find(|value| value.snapshot().run_id() == candidate)
                .expect("candidate observation")
                .snapshot()
                .deliverable()
                .is_some()
        );
        encode(AppResponsePayload::ProductRunObservations(observations));
        let exact =
            service.query_observations(ProductRunQuery::exact(candidate)).expect("exact candidate");
        assert!(exact[0].snapshot().deliverable().is_some());
        assert!(exact[0].settlement().is_some());
        encode(AppResponsePayload::ProductRunObservations(exact));
        service.shutdown(Duration::from_secs(5)).await;
    });
}

fn encode(payload: AppResponsePayload) {
    let context = ProtocolContext::new(
        ProtocolId::new([1; 16]).expect("protocol"),
        ProtocolVersion::new(1, 0).expect("version"),
        SessionId::new([2; 16]).expect("session"),
    );
    let message = AppMessage::Response(AppResponseEnvelope::new(
        context,
        RequestId::new([3; 16]).expect("request"),
        CorrelationId::new([4; 16]).expect("correlation"),
        payload,
    ));
    let bytes =
        encode_app_message(&message, AppProtocolLimits::PRODUCTION).expect("encodable response");
    assert_eq!(
        peritus_app_protocol::decode_app_message(&bytes, AppProtocolLimits::PRODUCTION)
            .expect("decodable response"),
        message
    );
}
