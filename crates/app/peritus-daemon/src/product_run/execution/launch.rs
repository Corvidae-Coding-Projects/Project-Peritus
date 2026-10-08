//! Owned asynchronous run launch and provider/runtime composition.

use super::runtime;
use crate::product_run::{ProductRunRequest, ProductRunService, ProductRunServiceError};
use peritus_product_runner::{
    ConversationView, ProductDeliveryScope, ProductRunInput, ProductRunResume, ProductRunner,
    RoleProviders, RunObserver,
};
use peritus_provider_core::CancellationToken;
use peritus_types::RunId;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[cfg(test)]
pub struct FinishBarrier {
    reached: tokio::sync::Notify,
    release: tokio::sync::Notify,
}

#[cfg(test)]
impl FinishBarrier {
    pub async fn reached(&self) {
        self.reached.notified().await;
    }

    pub fn release(&self) {
        self.release.notify_one();
    }
}

#[cfg(test)]
static FINISH_BARRIERS: std::sync::LazyLock<
    std::sync::Mutex<std::collections::BTreeMap<[u8; 16], Arc<FinishBarrier>>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::BTreeMap::new()));

struct RunOwnerGuard {
    service: ProductRunService,
    run: RunId,
    attempt: Arc<AtomicBool>,
}

impl Drop for RunOwnerGuard {
    fn drop(&mut self) {
        self.service.retire_run_cancellation(self.run, &self.attempt);
    }
}

#[cfg(test)]
pub fn inject_finish_barrier(run_id: RunId) -> Arc<FinishBarrier> {
    let barrier = Arc::new(FinishBarrier {
        reached: tokio::sync::Notify::new(),
        release: tokio::sync::Notify::new(),
    });
    FINISH_BARRIERS
        .lock()
        .expect("finish barrier lock")
        .insert(run_id.into_bytes(), Arc::clone(&barrier));
    barrier
}

#[cfg(test)]
async fn pause_before_finish(run_id: RunId) {
    let barrier =
        FINISH_BARRIERS.lock().ok().and_then(|mut values| values.remove(&run_id.into_bytes()));
    if let Some(barrier) = barrier {
        barrier.reached.notify_one();
        barrier.release.notified().await;
    }
}

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
    ) -> peritus_provider_core::BoxFuture<'_, Result<(), ProductRunServiceError>> {
        Box::pin(async move {
            let mut tasks = self.inner.tasks.lock().await;
            if !tasks.accepting {
                return Err(ProductRunServiceError::Unavailable);
            }
            let mut active = Vec::with_capacity(tasks.owners.len());
            for owner in std::mem::take(&mut tasks.owners) {
                if owner.is_finished() {
                    match owner.await {
                        Ok(Ok(())) => {}
                        Ok(Err(detail)) => tasks.record_owner_failure(detail),
                        Err(error) => tasks.record_owner_failure(error.to_string()),
                    }
                } else {
                    active.push(owner);
                }
            }
            tasks.owners = active;
            let service = self.clone();
            let run_id = request.run_id();
            let trace_path = self.inner.directory.join(format!("{}.trace", run_hex(run_id)));
            let observer: RunObserver = Arc::new(move |update| service.observe(run_id, update));
            let service = self.clone();
            // Keep the registry locked across handle creation and launch publication. Admission
            // may observe a retained owner while this short section runs, but it can call the
            // owner live only after the task handle exists.
            let ownership = self
                .inner
                .run_cancellations
                .lock()
                .map_err(|_| ProductRunServiceError::Unavailable)?;
            let cancellation = ownership
                .get(&run_id)
                .filter(|owner| {
                    Arc::ptr_eq(&owner.attempt_cancelled, &cancelled)
                        && owner.active.load(Ordering::Acquire)
                        && !owner.user_requested.load(Ordering::Acquire)
                })
                .ok_or(ProductRunServiceError::InvalidState)?;
            let goal_reconciliation = cancellation
                .reconciliation
                .as_ref()
                .cloned()
                .ok_or(ProductRunServiceError::InvalidState)?;
            // The guard belongs to the task future before it is spawned. Dropping an
            // unpolled task must retire its exact attempt just like a completed task.
            let owner = RunOwnerGuard {
                service: service.clone(),
                run: run_id,
                attempt: Arc::clone(&cancelled),
            };
            let task = tokio::spawn(async move {
                let _owner = owner;
                let finish_attempt = Arc::clone(&cancelled);
                let runtime_handle = tokio::runtime::Handle::current();
                let runner_service = service.clone();
                let runner = tokio::task::spawn_blocking(move || {
                    let folder = runner_service.inner.folders.get(&request.workspace_id());
                    let command_runtime = runtime::open(&runner_service, &request, &workspace_root)?;
                    let interaction_mode =
                        runner_service.inner.records.read().ok().and_then(|records| {
                            records.get(&run_id).map(|record| record.interaction.mode)
                        });
                    let conversation: Arc<dyn ConversationView> =
                        runner_service.live_conversation(run_id)?;
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
                    runtime_handle.block_on(runner_service.with_goal_clock(
                        run_id,
                        cancelled,
                        goal_reconciliation,
                        execution,
                    ))
                });
                let terminal_result = match runner.await {
                    Ok(result) => {
                        #[cfg(test)]
                        pause_before_finish(run_id).await;
                        service
                            .finish_async(run_id, Arc::clone(&finish_attempt), result)
                            .await
                    }
                    Err(error) => {
                        let detail = error.to_string();
                        service
                            .finish_worker_failure(
                                run_id,
                                Arc::clone(&finish_attempt),
                                "run product-run worker",
                                detail.clone(),
                            )
                            .await;
                        Err(format!("product-run worker failed: {detail}"))
                    }
                };
                // Inspect the exact pending-input state while this attempt still owns its
                // conversation/run reconciliation scope. An unavailable store is not absence.
                let pending_service = service.clone();
                let pending_attempt = Arc::clone(&finish_attempt);
                let pending = ProductRunService::await_blocking_owner(
                    "inspect pending product-run input",
                    move || {
                        pending_service.pending_interactive_input(run_id, &pending_attempt)
                    },
                )
                .await
                .map_err(|error| error.describe());
                let seal_service = service.clone();
                let seal_attempt = Arc::clone(&finish_attempt);
                let sealed = ProductRunService::await_blocking_owner(
                    "seal completed product-run owner",
                    move || seal_service.seal_run_owner(run_id, &seal_attempt),
                )
                .await
                .map_err(|error| error.describe());
                match (terminal_result, sealed) {
                    (Ok(()), Ok(())) => {}
                    (Err(terminal), Ok(())) => return Err(terminal),
                    (Ok(()), Err(seal)) => {
                        return Err(format!("product-run owner retirement failed: {seal}"));
                    }
                    (Err(terminal), Err(seal)) => {
                        return Err(format!(
                            "{terminal}; product-run owner retirement failed: {seal}"
                        ));
                    }
                }
                let pending = pending.map_err(|error| {
                    format!("pending product-run input remains unknown: {error}")
                })?;
                if pending {
                    service.retry(run_id).await.map_err(|error| error.describe())?;
                }
                Ok(())
            });
            cancellation.launched.store(true, Ordering::Release);
            drop(ownership);
            tasks.owners.push(task);
            Ok(())
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
