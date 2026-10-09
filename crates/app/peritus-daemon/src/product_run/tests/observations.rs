//! A running task beside an unqualified candidate must not disconnect any client.
use super::*;
use peritus_app_protocol::AppResponsePayload;
use peritus_app_protocol::{ProductRunOperationKind, ProductRunOperationState};

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
        let binding = service
            .query_interaction_binding(
                service.test_actor(candidate).expect("run owner"),
                peritus_app_protocol::ProductInteractionQuery::new(candidate),
            )
            .expect("durable follow-up destination");
        assert_eq!(binding.interaction().snapshot().run_id(), candidate);
        assert_eq!(binding.conversation().workspace(), workspace);
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

#[test]
fn uncertain_command_requires_explicit_review_and_never_admits_blind_retry() {
    interaction::block_on(async {
        let repository = repository();
        let state = tempfile::tempdir().expect("state");
        let writer = scripted(0xc1, "writer", Vec::new());
        let reviewer = scripted(0xc2, "reviewer", Vec::new());
        let fixer = scripted(0xc3, "fixer", Vec::new());
        let workspace = WorkspaceId::new([0xc4; 16]).expect("workspace");
        let run = RunId::new([0xc5; 16]).expect("run");
        let service =
            service(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
        service
            .start(
                ProductRunRequest::new(
                    run,
                    workspace,
                    ProductProviderSelection::new(
                        writer.profile.profile_id(),
                        reviewer.profile.profile_id(),
                        fixer.profile.profile_id(),
                    ),
                    "Exercise command recovery.".to_owned(),
                )
                .expect("request"),
            )
            .await
            .expect("start");
        let terminal = wait_for_terminal(&service, run).await;
        {
            let mut records = service.inner.records.write().expect("records");
            let record = records.get_mut(&run).expect("record");
            record.snapshot = crate::product_run::snapshot::replace_snapshot(
                &terminal,
                ProductRunPhase::RecoveryRequired,
                "Interrupted after command admission",
                terminal.summary(),
            )
            .expect("recovery snapshot");
            record.interruption_cause = "Command completion was not observed.".to_owned();
            crate::product_run::persistence::persist_record(&service.inner.directory, record)
                .expect("persist recovery state");
        }
        let effects = service
            .inner
            .directory
            .join(format!("{:032x}.trace", u128::from_be_bytes(run.into_bytes())))
            .with_extension("effects.bin");
        let scope = format!(
            "peritus-{:032x}-writer-1-revision-1-invocation-1-test",
            u128::from_be_bytes(run.into_bytes())
        );
        let receipt = serde_json::json!({
            "version": 1,
            "scope": scope,
            "ordinal": 1,
            "call_id": "call-1",
            "tool": "run_command",
            "request_sha256": "00",
            "state": "started",
        });
        let payload = serde_json::to_vec(&receipt).expect("receipt");
        let mut bytes =
            u64::try_from(payload.len()).expect("receipt length").to_le_bytes().to_vec();
        bytes.extend(payload);
        fs::write(&effects, bytes).expect("write interrupted receipt");

        let observed =
            service.query_observations(ProductRunQuery::exact(run)).expect("query unknown command");
        let operation = observed[0].snapshot().operation();
        assert_eq!(operation.kind(), ProductRunOperationKind::Command);
        assert_eq!(operation.state(), ProductRunOperationState::OutcomeUnknown);
        assert!(operation.legal_controls().acknowledge());
        assert!(!operation.legal_controls().retry());
        let before = fs::read(&effects).expect("receipt before rejected retry");
        assert!(
            service
                .control(ProductRunControl::new(run, ProductRunControlAction::Retry))
                .await
                .is_err()
        );
        assert_eq!(fs::read(&effects).expect("receipt after rejected retry"), before);

        let acknowledged = service
            .control(ProductRunControl::new(run, ProductRunControlAction::Acknowledge))
            .await
            .expect("review unknown command outcome");
        assert_eq!(acknowledged.operation().kind(), ProductRunOperationKind::Execution);
        assert_eq!(acknowledged.operation().state(), ProductRunOperationState::RecoveryRequired);
        assert!(acknowledged.operation().legal_controls().retry());
        assert!(!acknowledged.operation().legal_controls().acknowledge());
        assert!(acknowledged.status().contains("remains unknown"));
        assert!(acknowledged.operation().uncertainty().contains("mutations"));
        let reviewed_effects =
            peritus_product_runner::uncertain_effects(&effects).expect("inspect reviewed");
        assert_eq!(reviewed_effects.len(), 1);
        assert_eq!(
            reviewed_effects[0].state(),
            peritus_product_runner::UncertainEffectState::Reviewed
        );
        assert_eq!(reviewed_effects[0].requirements_revision(), Some(1));
        service.shutdown(Duration::from_secs(5)).await;
    });
}
