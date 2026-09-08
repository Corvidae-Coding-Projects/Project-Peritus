//! Regression: a confirmed selection must reach actual requests, not only a UI field.
use super::*;
use peritus_app_protocol::{
    ProductInteractionMode, ProductInteractionRequest, ProductModelChoice, ProductModelUpdate,
    ProductRoleModels, ProductRunConversationQuery,
};
use peritus_model_protocol::{ModelName, ModelRequest, ProviderProfile};
use peritus_provider_core::{BoxFuture, CancellationToken, OwnedModelStream, ProviderCoreError};
use tokio::sync::Notify;

struct Selectable {
    base: Arc<ScriptedProvider>,
    selected: Arc<ScriptedProvider>,
    started: Notify,
    release: Notify,
}
impl ModelProvider for Selectable {
    fn profile(&self) -> &ProviderProfile {
        self.base.profile()
    }
    fn select_model(&self, model: ModelName) -> Result<Arc<dyn ModelProvider>, ProviderCoreError> {
        if &model == self.selected.profile().model() {
            Ok(self.selected.clone())
        } else {
            Err(ProviderCoreError::configuration("fixture", "unsupported selected model"))
        }
    }
    fn start(
        &self,
        request: ModelRequest,
        cancellation: CancellationToken,
    ) -> BoxFuture<'_, Result<OwnedModelStream, ProviderCoreError>> {
        Box::pin(async move {
            peritus_provider_core::validate_request_profile(self.profile(), &request)?;
            self.started.notify_one();
            self.release.notified().await;
            self.base.start(request, cancellation).await
        })
    }
}

fn choices(mode: ProductInteractionMode, id: &str) -> ProductRoleModels {
    let selected = ProductModelChoice::new(id.to_owned(), true).expect("model");
    if mode == ProductInteractionMode::Review {
        ProductRoleModels::new(
            ProductModelChoice::default(),
            selected,
            ProductModelChoice::default(),
        )
    } else {
        ProductRoleModels::new(
            selected,
            ProductModelChoice::default(),
            ProductModelChoice::default(),
        )
    }
}

#[test]
fn active_selection_changes_the_next_request_without_restarting_or_losing_context() {
    interaction::block_on(async {
        for mode in [
            ProductInteractionMode::Chat,
            ProductInteractionMode::Plan,
            ProductInteractionMode::Review,
        ] {
            for direct_folder in [false, true] {
                active_scenario(mode, direct_folder).await;
            }
        }
    });
}

async fn active_scenario(mode: ProductInteractionMode, direct_folder: bool) {
    let repository = repository();
    let state = tempfile::tempdir().expect("state");
    let base = scripted(
        0x91,
        "sol",
        vec![support::named_tool_response("workspace_list", br#"{"path":"","depth":1}"#.to_vec())],
    );
    let selected = scripted(
        0x92,
        "astra",
        vec![support::text_response(b"Selected model received the conversation.")],
    );
    let provider = Arc::new(Selectable {
        base: base.clone(),
        selected: selected.clone(),
        started: Notify::new(),
        release: Notify::new(),
    });
    let (mut service, workspace, run) =
        scenario_service(state.path(), repository.path(), &base, direct_folder);
    Arc::get_mut(&mut service.inner)
        .expect("exclusive")
        .providers
        .insert(base.profile().profile_id(), provider.clone());
    let selection = ProductProviderSelection::new(
        base.profile().profile_id(),
        base.profile().profile_id(),
        base.profile().profile_id(),
    );
    service
        .interact(ProductInteractionRequest::new(
            ProductRunRequest::new(
                run,
                workspace,
                selection,
                "Inspect this workspace and explain it".to_owned(),
            )
            .expect("request"),
            mode,
            ProductRoleModels::default(),
        ))
        .await
        .expect("start");
    tokio::time::timeout(Duration::from_secs(5), provider.started.notified())
        .await
        .expect("old model in flight");
    let before = service.query_interaction(ProductRunConversationQuery::new(run)).expect("before");
    let models = choices(mode, selected.profile().model().as_str());
    let updated = service
        .update_models(&ProductModelUpdate::new(run, models.clone()))
        .await
        .expect("active update");
    assert_eq!(updated.models(), &models);
    assert_eq!(updated.received(), before.received(), "selection must not invent user input");
    assert_eq!(updated.incorporated(), before.incorporated());
    assert!(!updated.snapshot().phase().terminal());
    assert!(selected.requests.lock().expect("requests").is_empty(), "in-flight turn is unchanged");
    let invalid =
        service.update_models(&ProductModelUpdate::new(run, choices(mode, "unsupported"))).await;
    assert!(invalid.is_err());
    assert_eq!(
        service
            .query_interaction(ProductRunConversationQuery::new(run))
            .expect("unchanged")
            .models(),
        &models
    );
    let restored =
        super::super::persistence::load_records(&service.inner.directory).expect("durable");
    assert_eq!(
        &restored.get(&run).expect("run").interaction.as_ref().expect("options").models,
        &models
    );
    provider.release.notify_one();
    let terminal = wait_for_terminal(&service, run).await;
    assert_eq!(terminal.phase(), ProductRunPhase::WaitingForUser, "{}", terminal.summary());
    assert_eq!(base.requests.lock().expect("old requests").len(), 1);
    {
        let requests = selected.requests.lock().expect("selected requests");
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].model(), selected.profile().model());
        peritus_provider_core::validate_request_profile(selected.profile(), &requests[0])
            .expect("exact selected binding");
        assert!(
            requests[0]
                .messages()
                .iter()
                .any(|message| message.role() == peritus_model_protocol::Role::Tool),
            "tool history survives selection"
        );
    }
    let idle = service
        .update_models(&ProductModelUpdate::new(run, ProductRoleModels::default()))
        .await
        .expect("idle reset");
    assert_eq!(idle.snapshot().phase(), ProductRunPhase::WaitingForUser);
    assert_eq!(
        base.requests.lock().expect("requests").len(),
        1,
        "idle selection does not start work"
    );
    assert_failed_persistence_keeps_prior_selection(&service, run, models).await;
    service.shutdown(Duration::from_secs(5)).await;
}

async fn assert_failed_persistence_keeps_prior_selection(
    service: &ProductRunService,
    run: RunId,
    models: ProductRoleModels,
) {
    let saved = fs::read_dir(&service.inner.directory)
        .expect("state directory")
        .map(|entry| entry.expect("entry").path())
        .find(|path| path.extension().is_some_and(|extension| extension == "json"))
        .expect("one saved run");
    let before = fs::read(&saved).expect("saved bytes");
    fs::create_dir(saved.with_extension("json.new")).expect("block temporary file publication");
    assert!(service.update_models(&ProductModelUpdate::new(run, models)).await.is_err());
    assert_eq!(fs::read(saved).expect("prior durable bytes"), before);
    let records = service.inner.records.read().expect("records");
    assert_eq!(
        records.get(&run).expect("run").interaction.as_ref().expect("options").models,
        ProductRoleModels::default()
    );
}

fn scenario_service(
    state: &std::path::Path,
    root: &std::path::Path,
    base: &Arc<ScriptedProvider>,
    direct_folder: bool,
) -> (ProductRunService, WorkspaceId, RunId) {
    if direct_folder {
        let (service, request) = folder::folder_service(root, base, true);
        (service, request.workspace_id(), request.run_id())
    } else {
        let workspace = WorkspaceId::new([0x93; 16]).expect("workspace");
        let run = RunId::new([0x94; 16]).expect("run");
        (service(state, root, workspace, [base, base, base]), workspace, run)
    }
}
