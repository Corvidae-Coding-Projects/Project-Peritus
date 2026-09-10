//! Owned asynchronous run launch and provider/runtime composition.

use super::runtime;
use crate::product_run::{ProductRunService, SharedConversation};
use peritus_app_protocol::ProductRunRequest;
use peritus_product_runner::{
    ConversationView, PRODUCT_RUN_MAX_ELAPSED, ProductDeliveryScope, ProductRunInput,
    ProductRunResume, ProductRunner, RoleProviders, RunObserver,
};
use peritus_provider_core::CancellationToken;
use peritus_types::RunId;
use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
};

impl ProductRunService {
    #[allow(
        clippy::too_many_arguments,
        reason = "daemon execution inputs stay explicit at the task ownership boundary"
    )]
    pub(in crate::product_run) fn spawn(
        &self,
        request: ProductRunRequest,
        workspace_root: PathBuf,
        providers: RoleProviders,
        cancelled: Arc<AtomicBool>,
        provider_cancellation: CancellationToken,
        conversation: Arc<SharedConversation>,
        finding_state: String,
        resume: Option<ProductRunResume>,
    ) -> peritus_provider_core::BoxFuture<'_, ()> {
        Box::pin(async move {
            let service = self.clone();
            let run_id = request.run_id();
            let trace_path = self.inner.directory.join(format!("{}.trace", run_hex(run_id)));
            let observer: RunObserver = Arc::new(move |update| service.observe(run_id, update));
            let service = self.clone();
            let task = tokio::spawn(async move {
                let folder = service.inner.folders.get(&request.workspace_id());
                let command_runtime = match runtime::open(&service, &request, &workspace_root) {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        service.finish(run_id, Err(error));
                        return;
                    }
                };
                let interaction_mode = service.inner.records.read().ok().and_then(|records| {
                    records.get(&run_id).and_then(|record| {
                        record.interaction.as_ref().map(|interaction| interaction.mode)
                    })
                });
                let conversation: Arc<dyn ConversationView> = if interaction_mode.is_some() {
                    service.live_conversation(run_id)
                } else {
                    conversation
                };
                let input = ProductRunInput {
                    workspace_kind: peritus_product_runner::ProductWorkspaceKind::Managed,
                    run_id,
                    workspace_id: request.workspace_id(),
                    workspace_root,
                    trace_path,
                    command_runtime,
                    finding_state,
                    task: request.task().to_owned(),
                    max_elapsed: PRODUCT_RUN_MAX_ELAPSED,
                    delivery_scope: ProductDeliveryScope::WorkspaceChanges,
                    conversation,
                    providers,
                    cancelled,
                    provider_cancellation,
                    resume,
                };
                let mode = match interaction_mode {
                    Some(peritus_app_protocol::ProductInteractionMode::Chat) => {
                        Some(peritus_product_runner::ConversationMode::Chat)
                    }
                    Some(peritus_app_protocol::ProductInteractionMode::Plan) => {
                        Some(peritus_product_runner::ConversationMode::Plan)
                    }
                    Some(peritus_app_protocol::ProductInteractionMode::Review) => {
                        Some(peritus_product_runner::ConversationMode::Review)
                    }
                    Some(peritus_app_protocol::ProductInteractionMode::Build) | None => None,
                };
                let result = match mode {
                    Some(mode) if folder.is_some() => {
                        let folder = folder.expect("folder selected");
                        ProductRunner::converse_folder(
                            input,
                            mode,
                            folder.writable(),
                            folder.protected_paths(),
                            observer,
                        )
                        .await
                    }
                    Some(mode) => ProductRunner::converse(input, mode, observer).await,
                    None => ProductRunner::run(input, observer).await,
                };
                service.finish(run_id, result);
                if service.pending_interactive_input(run_id) {
                    let _ = service.retry(run_id).await;
                }
            });
            let mut tasks = self.inner.tasks.lock().await;
            tasks.retain(|existing| !existing.is_finished());
            tasks.push(task);
        })
    }
}

pub(super) fn run_hex(run_id: RunId) -> String {
    run_id.as_bytes().iter().fold(String::new(), |mut value, byte| {
        use core::fmt::Write as _;
        let _ = write!(value, "{byte:02x}");
        value
    })
}
