//! Owned asynchronous run launch and provider/runtime composition.

use super::runtime;
use crate::product_run::{ProductRunRequest, ProductRunService};
use peritus_product_runner::{
    ConversationView, ProductDeliveryScope, ProductRunInput, ProductRunResume, ProductRunner,
    RoleProviders, RunObserver,
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
        finding_state: String,
        resume: Option<ProductRunResume>,
    ) -> peritus_provider_core::BoxFuture<'_, ()> {
        Box::pin(async move {
            let service = self.clone();
            let run_id = request.run_id();
            let trace_path = self.inner.directory.join(format!("{}.trace", run_hex(run_id)));
            let observer: RunObserver = Arc::new(move |update| service.observe(run_id, update));
            let service = self.clone();
            let mut tasks = loop {
                let tasks = self.inner.tasks.lock().await;
                let recovery_active = self
                    .inner
                    .command_recoveries
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .contains(&run_id);
                if recovery_active {
                    drop(tasks);
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                    continue;
                }
                break tasks;
            };
            tasks.retain(|(_, existing)| !existing.is_finished());
            let execution = async move {
                let folder = service.inner.workspaces.folder(&request.workspace_id());
                let command_runtime = match runtime::open(&service, &request, &workspace_root) {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        service.finish(run_id, Err(error));
                        return;
                    }
                };
                let interaction_mode =
                    service.inner.records.read().ok().and_then(|records| {
                        records.get(&run_id).map(|record| record.interaction.mode)
                    });
                let conversation: Arc<dyn ConversationView> = service.live_conversation(run_id);
                let input = ProductRunInput {
                    workspace_kind: peritus_product_runner::ProductWorkspaceKind::Managed,
                    run_id,
                    workspace_id: request.workspace_id(),
                    workspace_root,
                    trace_path,
                    command_runtime,
                    finding_state,
                    task: request.execution_task().to_owned(),
                    max_elapsed: None,
                    delivery_scope: ProductDeliveryScope::WorkspaceChanges,
                    conversation,
                    providers,
                    cancelled: Arc::clone(&cancelled),
                    provider_cancellation: provider_cancellation.clone(),
                    resume,
                };
                let mode = match interaction_mode {
                    Some(peritus_app_protocol::ProductInteractionMode::Chat) => {
                        Some(peritus_product_runner::ConversationMode::Chat)
                    }
                    Some(peritus_app_protocol::ProductInteractionMode::Plan) => {
                        Some(peritus_product_runner::ConversationMode::Plan)
                    }
                    Some(peritus_app_protocol::ProductInteractionMode::Design) => {
                        Some(peritus_product_runner::ConversationMode::Design)
                    }
                    Some(peritus_app_protocol::ProductInteractionMode::Review) => {
                        Some(peritus_product_runner::ConversationMode::Review)
                    }
                    Some(peritus_app_protocol::ProductInteractionMode::Build) | None => None,
                };
                let execution = async {
                    match mode {
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
                    }
                };
                let result = Box::pin(service.with_goal_clock(
                    run_id,
                    cancelled,
                    provider_cancellation,
                    execution,
                ))
                .await;

                service.finish(run_id, result);
                if service.pending_interactive_input(run_id) {
                    let _ = service.retry(run_id).await;
                }
            };
            // Prompt planning, local history and host tools include synchronous CPU/file
            // work. Keep that work off the async workers that accept connections and
            // answer heartbeats. The same runtime still owns provider/process I/O, and
            // this retained join handle plus the run's cancellation tokens own its life.
            let runtime = tokio::runtime::Handle::current();
            let task = tokio::task::spawn_blocking(move || runtime.block_on(execution));
            tasks.push((run_id, task));
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
