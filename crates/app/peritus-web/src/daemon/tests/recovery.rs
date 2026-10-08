use super::*;

#[tokio::test]
async fn lost_control_response_is_reconciled_without_reissuing_the_mutation() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let endpoint = root.join("recovery.sock");
    let listener = UnixListener::bind(&endpoint).unwrap();
    let app = isolated_app(root, endpoint.clone());
    let server = tokio::spawn(async move {
        let (mut stream, request) = receive_request(&listener).await;
        assert!(matches!(request.payload(), AppRequestPayload::QueryInteraction(_)));
        let run = RunId::new([1; 16]).unwrap();
        let provider = ProviderProfileId::new([2; 16]).unwrap();
        let baseline = ProductRunSnapshot::new(
            run,
            WorkspaceId::new([3; 16]).unwrap(),
            ProductProviderSelection::new(provider, provider, provider),
            ProductRunPhase::Writing,
            1,
            "Cancellation fixture".into(),
            "Writing".into(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            operation(run, ProductRunOperationState::Running),
        )
        .unwrap();
        let observation = ProductInteractionSnapshot::new(
            baseline,
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
        let (stream, request) = receive_request(&listener).await;
        assert!(matches!(request.payload(), AppRequestPayload::ControlProductRun(_)));
        drop(stream);
        let (mut stream, request) = receive_request(&listener).await;
        assert!(matches!(request.payload(), AppRequestPayload::QueryInteraction(_)));
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
            operation(run, ProductRunOperationState::Cancelled),
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
    let retained = reopened.operation("daemon:original").unwrap().unwrap();
    assert!(retained.input["frame"].as_str().unwrap().len() > 64);
    assert!(receipts::recorded(&reopened, "original", payload).await.unwrap_err().1);
    server.await.unwrap();
}

#[tokio::test]
async fn lost_workbench_response_is_reconciled_from_the_exact_domain_receipt() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let endpoint = root.join("workbench-recovery.sock");
    let listener = UnixListener::bind(&endpoint).unwrap();
    let app = isolated_app(root, endpoint.clone());
    let workspace = WorkspaceId::new([5; 16]).unwrap();
    let query = WorkbenchQuery::new(ConversationId::new([4; 16]).unwrap(), workspace);
    let command = WorkbenchCommand::new(
        peritus_app_protocol::ControlOperationId::new([6; 16]).unwrap(),
        query,
        0,
        WorkbenchIntent::CreateConversation(ConversationTitle::new("Only once".into()).unwrap()),
    );
    let expected = command.clone();
    let server_command = expected.clone();
    let server = tokio::spawn(async move {
        let (stream, request) = receive_request(&listener).await;
        assert_eq!(request.payload(), &AppRequestPayload::WorkbenchCommand(server_command.clone()));
        drop(stream);
        let (mut stream, request) = receive_request(&listener).await;
        assert_eq!(
            request.payload(),
            &AppRequestPayload::QueryWorkbenchReceipt(server_command.clone())
        );
        let receipt = WorkbenchReceipt::new(
            server_command.operation(),
            server_command.query(),
            1,
            peritus_types::Sha256Digest::new([7; 32]),
        )
        .unwrap();
        write_message(
            &mut stream,
            AppMessage::Response(AppResponseEnvelope::new(
                request.context(),
                request.request_id(),
                request.correlation_id(),
                AppResponsePayload::WorkbenchReceipt(receipt),
            )),
        )
        .await;
    });
    assert!(receipts::workbench_command(&app, "workbench", command.clone()).await.unwrap_err().1);
    let reopened = isolated_app(root, endpoint);
    let recovered = receipts::workbench_command(&reopened, "workbench", command).await.unwrap();
    assert!(matches!(recovered, AppResponsePayload::WorkbenchReceipt(receipt)
        if receipt.operation() == expected.operation() && receipt.accepted_revision() == 1));
    assert!(reopened.operation("daemon:workbench").unwrap().unwrap().result.is_some());
    server.await.unwrap();
}
