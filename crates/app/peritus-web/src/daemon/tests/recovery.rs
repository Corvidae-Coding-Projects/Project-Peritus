use super::*;

#[tokio::test]
async fn lost_control_response_is_reconciled_without_reissuing_the_mutation() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let endpoint = root.join("recovery.sock");
    let listener = UnixListener::bind(&endpoint).unwrap();
    let app = isolated_app(root, endpoint.clone());
    let server = tokio::spawn(async move {
        let (stream, request) = receive_request(&listener).await;
        assert!(matches!(request.payload(), AppRequestPayload::ControlProductRun(_)));
        drop(stream);
        let (mut stream, request) = receive_request(&listener).await;
        assert!(matches!(request.payload(), AppRequestPayload::QueryInteraction(_)));
        let run = RunId::new([1; 16]).unwrap();
        let provider = ProviderProfileId::new([2; 16]).unwrap();
        let snapshot = ProductRunSnapshot::new(
            run,
            WorkspaceId::new([3; 16]).unwrap(),
            ProductProviderSelection::new(provider, provider, provider),
            ProductRunPhase::Cancelled,
            1,
            "Cancelled fixture".into(),
            "Cancelled".into(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        )
        .unwrap();
        let observation = ProductInteractionSnapshot::new(
            snapshot,
            ProductInteractionMode::Chat,
            ProductRoleModels::default(),
            1,
            1,
            Vec::new(),
            None,
        )
        .unwrap();
        write_message(
            &mut stream,
            AppMessage::Response(AppResponseEnvelope::new(
                request.context(),
                request.request_id(),
                request.correlation_id(),
                AppResponsePayload::Interaction(observation),
            )),
        )
        .await;
    });
    let payload = AppRequestPayload::ControlProductRun(ProductRunControl::new(
        RunId::new([1; 16]).unwrap(),
        ProductRunControlAction::Cancel,
    ));
    let error = receipts::recorded(&app, "original", payload.clone()).await.unwrap_err();
    assert!(error.1);
    let reopened = isolated_app(root, endpoint);
    let recovered = receipts::observed(&reopened, "original").await.unwrap().unwrap();
    assert_eq!(recovered["run"]["phase"], "Cancelled");
    assert_eq!(recovered["recovered"], true);
    let retained = reopened.snapshot().unwrap();
    assert!(retained.operations["daemon:original"].input["frame"].as_str().unwrap().len() > 64);
    assert!(receipts::recorded(&reopened, "original", payload).await.unwrap_err().1);
    server.await.unwrap();
}

#[tokio::test]
async fn lost_interaction_response_is_reconciled_from_exact_revision_and_user_activity() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let endpoint = root.join("interaction-recovery.sock");
    let listener = UnixListener::bind(&endpoint).unwrap();
    let app = isolated_app(root, endpoint.clone());
    let run = RunId::new([4; 16]).unwrap();
    let workspace = WorkspaceId::new([5; 16]).unwrap();
    let provider = ProviderProfileId::new([6; 16]).unwrap();
    let providers = ProductProviderSelection::new(provider, provider, provider);
    let server = tokio::spawn(async move {
        let (mut stream, request) = receive_request(&listener).await;
        assert!(matches!(request.payload(), AppRequestPayload::QueryInteraction(_)));
        write_message(
            &mut stream,
            AppMessage::Response(AppResponseEnvelope::new(
                request.context(),
                request.request_id(),
                request.correlation_id(),
                AppResponsePayload::Error(peritus_app_protocol::AppProtocolError::new(
                    peritus_app_protocol::AppErrorCode::InvalidIdentifier,
                    None,
                )),
            )),
        )
        .await;
        let (stream, request) = receive_request(&listener).await;
        assert!(matches!(request.payload(), AppRequestPayload::Interact(_)));
        drop(stream);
        let (mut stream, request) = receive_request(&listener).await;
        assert!(matches!(request.payload(), AppRequestPayload::QueryInteraction(_)));
        let snapshot = ProductRunSnapshot::new(
            run,
            workspace,
            providers,
            ProductRunPhase::Queued,
            1,
            "Only once".into(),
            "Queued".into(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        )
        .unwrap();
        let observation = ProductInteractionSnapshot::new(
            snapshot,
            ProductInteractionMode::Chat,
            ProductRoleModels::default(),
            1,
            0,
            vec![
                ProductActivity::new(
                    1,
                    ProductActivityKind::User,
                    "Only once".into(),
                    String::new(),
                )
                .unwrap(),
            ],
            None,
        )
        .unwrap();
        write_message(
            &mut stream,
            AppMessage::Response(AppResponseEnvelope::new(
                request.context(),
                request.request_id(),
                request.correlation_id(),
                AppResponsePayload::Interaction(observation),
            )),
        )
        .await;
    });
    let payload = AppRequestPayload::Interact(ProductInteractionRequest::new(
        ProductRunRequest::new(run, workspace, providers, "Only once".into()).unwrap(),
        ProductInteractionMode::Chat,
        ProductRoleModels::default(),
    ));

    assert!(receipts::recorded(&app, "interaction", payload).await.unwrap_err().1);
    let reopened = isolated_app(root, endpoint);
    let recovered = receipts::observed(&reopened, "interaction").await.unwrap().unwrap();
    assert_eq!(recovered["received"], "1");
    assert_eq!(recovered["activities"][0]["text"], "Only once");
    assert_eq!(recovered["recovered"], true);
    server.await.unwrap();
}
