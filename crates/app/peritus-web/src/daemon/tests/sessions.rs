//! Cross-client conversation adoption and durable model updates over the native protocol.
use super::*;
use crate::sessions;
use peritus_app_protocol::ProductModelUpdate;

fn observation(
    run: RunId,
    workspace: WorkspaceId,
    models: ProductRoleModels,
) -> AppResponsePayload {
    let provider = ProviderProfileId::new([2; 16]).unwrap();
    let snapshot = ProductRunSnapshot::new(
        run,
        workspace,
        ProductProviderSelection::new(provider, provider, provider),
        ProductRunPhase::Queued,
        0,
        "CLI conversation".into(),
        "Queued".into(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
    )
    .unwrap();
    AppResponsePayload::Interaction(
        ProductInteractionSnapshot::new(
            snapshot,
            ProductInteractionMode::Plan,
            models,
            1,
            1,
            Vec::new(),
            None,
        )
        .unwrap(),
    )
}

#[tokio::test]
async fn imports_exact_cli_run_reopens_it_and_rejects_another_workspace() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().canonicalize().unwrap();
    let endpoint = root.join("session.sock");
    let listener = UnixListener::bind(&endpoint).unwrap();
    let app = isolated_app(&root, endpoint);
    let workspace = WorkspaceId::new([4; 16]).unwrap();
    let run = RunId::new([5; 16]).unwrap();
    std::fs::write(root.join("peritus-1.toml"), "providers = []\n").unwrap();
    std::fs::write(
        root.join("state-1.json"),
        serde_json::to_vec(&json!({"workspaces":{"recent":[{
            "workspace_id":hex(workspace.as_bytes()),"repository_root":root,"trust":"trusted"
        }]}}))
        .unwrap(),
    )
    .unwrap();
    let server = tokio::spawn(async move {
        for workspace in [workspace, workspace, WorkspaceId::new([6; 16]).unwrap()] {
            let (mut stream, request) = receive_request(&listener).await;
            assert!(
                matches!(request.payload(), AppRequestPayload::QueryInteraction(query) if query.run_id() == run)
            );
            write_message(
                &mut stream,
                AppMessage::Response(AppResponseEnvelope::new(
                    request.context(),
                    request.request_id(),
                    request.correlation_id(),
                    observation(run, workspace, ProductRoleModels::default()),
                )),
            )
            .await;
        }
    });
    let id = hex(run.as_bytes());
    let imported = sessions::open_run(&app, &id).await.unwrap();
    assert_eq!(imported["id"], id);
    assert_eq!(imported["title"], "CLI conversation");
    app.update(|state| {
        state.sessions.iter_mut().find(|s| s.id == id).unwrap().closed = true;
        state.projects[0].closed = true;
        Ok(())
    })
    .unwrap();
    assert_eq!(sessions::open_run(&app, &id).await.unwrap()["closed"], false);
    assert!(!app.snapshot().unwrap().projects[0].closed);
    assert_eq!(app.snapshot().unwrap().sessions.len(), 2);
    assert!(sessions::open_run(&app, &id).await.unwrap_err().0.contains("project"));
    server.await.unwrap();
}

#[tokio::test]
async fn model_update_and_recovered_local_save_retain_the_same_native_selection() {
    let temporary = tempfile::tempdir().unwrap();
    let endpoint = temporary.path().join("models.sock");
    let listener = UnixListener::bind(&endpoint).unwrap();
    let app = isolated_app(temporary.path(), endpoint);
    let session = app.snapshot().unwrap().sessions[0].id.clone();
    let run = RunId::new(bytes(&session).unwrap()).unwrap();
    let models = ProductRoleModels::new(
        ProductModelChoice::new("selected".into(), true)
            .unwrap()
            .with_effort(ProductModelEffort::High),
        ProductModelChoice::default(),
        ProductModelChoice::default(),
    );
    let expected = models.clone();
    let server = tokio::spawn(async move {
        let (mut stream, request) = receive_request(&listener).await;
        assert_eq!(
            request.payload(),
            &AppRequestPayload::UpdateModels(ProductModelUpdate::new(run, expected.clone()))
        );
        write_message(
            &mut stream,
            AppMessage::Response(AppResponseEnvelope::new(
                request.context(),
                request.request_id(),
                request.correlation_id(),
                observation(run, WorkspaceId::new([4; 16]).unwrap(), expected),
            )),
        )
        .await;
    });
    let input = json!({"command":"session-settings","operation":"models-op","session":session,"existing":true,"settings":{"models":{"writer":{"id":"selected","manual":true,"effort":"high"}},"providers":{"writer":""}}});
    let result = sessions::configure(&app, &input).await.unwrap();
    assert_eq!(result["conversation"]["models"], model_values(&models));
    assert_eq!(result["conversation"]["mode"], "plan");
    // Simulate loss between the durable native response and the presentation save.
    app.update(|state| {
        state.sessions[0].settings = sessions::Settings::default();
        state.operations.insert(
            "models-op".into(),
            crate::state::Operation { input: input.clone(), result: None },
        );
        Ok(())
    })
    .unwrap();
    let recovered = crate::operations::observe(&app, "models-op").unwrap();
    assert_eq!(recovered["result"], result);
    assert_eq!(app.session(&session).unwrap().settings.models["writer"].id, "selected");
    server.await.unwrap();
}
