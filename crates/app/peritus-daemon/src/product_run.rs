//! Daemon-owned product-run registry, persistence, and execution admission.

mod catalog;
mod construction;
mod continuation;
mod conversation;
mod deliverable;
mod doctor;
mod error;
mod execution;
mod interaction;
mod library;
mod lifecycle;
mod permissions;
mod persistence;
mod progress;
mod recovery;
mod snapshot;
mod workbench;

#[cfg(test)]
mod tests;

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, RwLock, atomic::AtomicBool},
};

use peritus_app_protocol::{
    AppResponsePayload, ControlOperationId, ProductConversationMessage, ProductConversationRole,
    ProductProviderSelection, ProductRunControl, ProductRunControlAction, ProductRunConversation,
    ProductRunConversationQuery, ProductRunQuery, ProductRunRequest, ProductRunSnapshot,
    WorkbenchResultPage,
};
use peritus_process::ProcessStore;
use peritus_product_runner::{CommandRuntime, PreviewLaunch, ProductRunResume, RoleProviders};
use peritus_provider_core::{CancellationToken, ModelProvider};
use peritus_run_settlement::{CandidateCheckpoint, RunSettlement};
use peritus_types::{ProviderProfileId, RunId, WorkspaceId};
use tokio::{sync::Mutex, task::JoinHandle};

use crate::{DaemonComponents, DaemonError, startup::workspace::WorkspaceCatalog};

use conversation::SharedConversation;
pub use error::ProductRunServiceError;
use error::{filesystem, invalid};
use persistence::{load_records, persist_record};
use progress::RunProgress;
use recovery::reconcile_restored_candidates;
use snapshot::{
    initial_snapshot, live_snapshot, project_collection, project_snapshot, replace_snapshot,
    workspace_has_active_run,
};

#[derive(Clone)]
pub struct ProductRunService {
    inner: Arc<Inner>,
}

struct Inner {
    controls: std::sync::Mutex<Option<crate::product_control::ControlStore>>,
    control_store: peritus_journal::StoreId,
    directory: PathBuf,
    records: RwLock<BTreeMap<RunId, RunRecord>>,
    providers: BTreeMap<ProviderProfileId, Arc<dyn ModelProvider>>,
    automatic_provider_failover: bool,
    local_context: peritus_product_runner::LocalContextConfig,
    workspaces: BTreeMap<WorkspaceId, PathBuf>,
    folders: BTreeMap<WorkspaceId, crate::config::FolderDeclaration>,
    processes: ProcessStore,
    tasks: Mutex<Vec<JoinHandle<()>>>,
    model_catalogs: Mutex<BTreeMap<ProviderProfileId, peritus_app_protocol::ProductModelCatalog>>,
    image_decodes: Arc<tokio::sync::Semaphore>,
    host_permissions: permissions::HostPermissionCatalog,
    preview_processes: std::sync::Mutex<BTreeMap<ControlOperationId, PreviewProcess>>,
    preview_capture: PreviewCaptureHost,
}

#[derive(Clone)]
struct PreviewProcess {
    runtime: CommandRuntime,
    launch: PreviewLaunch,
}

#[derive(Clone)]
struct PreviewCaptureHost {
    display: Option<String>,
    program: Option<PathBuf>,
}

#[derive(Clone, Default)]
struct PreviewAggregate {
    page: Option<WorkbenchResultPage>,
    operations: BTreeMap<ControlOperationId, PreviewOperationRecord>,
    outputs: BTreeMap<ControlOperationId, String>,
}

#[derive(Clone, Copy)]
struct PreviewOperationRecord {
    fingerprint: peritus_types::Sha256Digest,
    accepted_revision: u64,
    result_sequence: u64,
    completed_sequence: u64,
}

struct RunRecord {
    interaction: Option<interaction::InteractionOptions>,
    request: ProductRunRequest,
    snapshot: ProductRunSnapshot,
    cancelled: Arc<AtomicBool>,
    user_cancelled: bool,
    provider_cancellation: CancellationToken,
    conversation: Arc<SharedConversation>,
    finding_state: String,
    progress: RunProgress,
    checkpoint: Option<CandidateCheckpoint>,
    settlement: Option<RunSettlement>,
    resume: Option<ProductRunResume>,
    remaining_work: Vec<String>,
    interruption_cause: String,
    candidate_actionable: bool,
    preview: PreviewAggregate,
}

impl ProductRunService {
    pub(super) async fn start(
        &self,
        request: ProductRunRequest,
    ) -> Result<ProductRunSnapshot, ProductRunServiceError> {
        self.start_configured(request, None).await
    }

    async fn start_configured(
        &self,
        request: ProductRunRequest,
        mut interaction: Option<interaction::InteractionOptions>,
    ) -> Result<ProductRunSnapshot, ProductRunServiceError> {
        self.validate_workspace_mode(request.workspace_id(), interaction.as_ref())?;
        let providers =
            self.resolve_selected_providers(request.providers(), interaction.as_ref())?;
        let workspace_root = self
            .inner
            .workspaces
            .get(&request.workspace_id())
            .cloned()
            .ok_or(ProductRunServiceError::WorkspaceUnavailable)?;
        let snapshot = initial_snapshot(&request)?;
        let conversation = SharedConversation::new(
            request.run_id(),
            vec![
                ProductConversationMessage::new(
                    ProductConversationRole::User,
                    request.task().to_owned(),
                )
                .map_err(|_| ProductRunServiceError::InvalidMessage)?,
            ],
        )?;
        let cancelled = Arc::new(AtomicBool::new(false));
        let provider_cancellation = CancellationToken::new();
        if let Some(options) = interaction.as_mut() {
            options.append(
                peritus_app_protocol::ProductActivityKind::User,
                request.task(),
                "Input 1 received",
            )?;
        }
        {
            let mut records =
                self.inner.records.write().map_err(|_| ProductRunServiceError::Unavailable)?;
            if let Some(staged) = records.get(&request.run_id()) {
                let proposed = interaction.as_ref().and_then(|options| options.workbench.as_ref());
                let existing =
                    staged.interaction.as_ref().and_then(|options| options.workbench.as_ref());
                let replace_staged = proposed.is_some()
                    && proposed == existing
                    && staged.snapshot.phase()
                        == peritus_app_protocol::ProductRunPhase::RecoveryRequired
                    && self
                        .with_controls(false, |store| {
                            store.resolve(proposed.expect("checked proposed binding"))
                        })?
                        .is_none();
                if !replace_staged {
                    return Err(ProductRunServiceError::Duplicate);
                }
            }
            if workspace_has_active_run(&records, request.workspace_id(), None) {
                return Err(ProductRunServiceError::InvalidState);
            }
            records.insert(
                request.run_id(),
                RunRecord {
                    interaction,
                    request: request.clone(),
                    snapshot: snapshot.clone(),
                    cancelled: Arc::clone(&cancelled),
                    user_cancelled: false,
                    provider_cancellation: provider_cancellation.clone(),
                    conversation: Arc::clone(&conversation),
                    finding_state: String::new(),
                    progress: RunProgress::default(),
                    checkpoint: None,
                    settlement: None,
                    resume: None,
                    remaining_work: Vec::new(),
                    interruption_cause: String::new(),
                    candidate_actionable: false,
                    preview: PreviewAggregate::default(),
                },
            );
            if let Err(error) = persist_record(
                &self.inner.directory,
                records.get(&request.run_id()).expect("inserted product run"),
            ) {
                records.remove(&request.run_id());
                return Err(error);
            }
            let start = records
                .get(&request.run_id())
                .and_then(|record| record.interaction.as_ref())
                .and_then(|options| options.workbench.as_ref());
            if let Some(operation) = start
                && let Err(error) = self.with_controls(false, |store| store.accept(operation))
            {
                // The staged record is in the fenced generation and cannot run without its C0
                // binding. Retain it on disk for diagnosis/recovery, but never spawn on failure.
                records.remove(&request.run_id());
                return Err(error.into());
            }
        }
        self.spawn(
            request,
            workspace_root,
            providers,
            cancelled,
            provider_cancellation,
            conversation,
            String::new(),
            None,
        )
        .await;
        Ok(snapshot)
    }

    fn validate_workspace_mode(
        &self,
        workspace_id: WorkspaceId,
        interaction: Option<&interaction::InteractionOptions>,
    ) -> Result<(), ProductRunServiceError> {
        if let Some(folder) = self.inner.folders.get(&workspace_id) {
            folder.verify().map_err(|_| ProductRunServiceError::WorkspaceUnavailable)?;
            if interaction.is_none_or(|options| {
                options.mode == peritus_app_protocol::ProductInteractionMode::Build
            }) {
                return Err(ProductRunServiceError::GitRequired);
            }
        }
        Ok(())
    }

    pub(super) async fn control(
        &self,
        control: ProductRunControl,
    ) -> Result<ProductRunSnapshot, ProductRunServiceError> {
        if self.governed_run(control.run_id())? {
            return Err(ProductRunServiceError::Control(
                peritus_product_runner::control::ControlError::UnsupportedSchema,
            ));
        }
        match control.action() {
            ProductRunControlAction::Cancel => self.cancel(control.run_id()),
            ProductRunControlAction::Retry => self.retry(control.run_id()).await,
            ProductRunControlAction::Accept
            | ProductRunControlAction::Commit
            | ProductRunControlAction::Export
            | ProductRunControlAction::Discard => {
                self.control_deliverable(control.run_id(), control.action())
            }
        }
    }

    pub(super) fn governed_run(&self, run: RunId) -> Result<bool, ProductRunServiceError> {
        let records = self.inner.records.read().map_err(|_| ProductRunServiceError::Unavailable)?;
        Ok(records.get(&run).is_some_and(|record| {
            record.interaction.as_ref().is_some_and(|options| options.workbench.is_some())
        }))
    }

    pub(super) fn query(
        &self,
        query: ProductRunQuery,
    ) -> Result<Vec<ProductRunSnapshot>, ProductRunServiceError> {
        let records = self.inner.records.read().map_err(|_| ProductRunServiceError::Unavailable)?;
        if let Some(run_id) = query.run_id() {
            return records
                .get(&run_id)
                .map(live_snapshot)
                .transpose()
                .map(|snapshot| snapshot.into_iter().collect());
        }
        records
            .values()
            .rev()
            .take(peritus_app_protocol::MAX_PRODUCT_RUNS)
            .map(live_snapshot)
            .collect()
    }

    pub(super) fn project(
        &self,
        snapshot: ProductRunSnapshot,
    ) -> Result<AppResponsePayload, ProductRunServiceError> {
        let records = self.inner.records.read().map_err(|_| ProductRunServiceError::Unavailable)?;
        let record = records.get(&snapshot.run_id()).ok_or(ProductRunServiceError::NotFound)?;
        project_snapshot(record, snapshot)
    }

    pub(super) fn project_many(
        &self,
        snapshots: Vec<ProductRunSnapshot>,
    ) -> Result<AppResponsePayload, ProductRunServiceError> {
        let records = self.inner.records.read().map_err(|_| ProductRunServiceError::Unavailable)?;
        project_collection(&records, snapshots)
    }

    pub(super) fn query_conversation(
        &self,
        query: ProductRunConversationQuery,
    ) -> Result<ProductRunConversation, ProductRunServiceError> {
        let records = self.inner.records.read().map_err(|_| ProductRunServiceError::Unavailable)?;
        records
            .get(&query.run_id())
            .ok_or(ProductRunServiceError::NotFound)?
            .conversation
            .snapshot()
    }

    fn resolve_providers(
        &self,
        selected: ProductProviderSelection,
    ) -> Result<RoleProviders, ProductRunServiceError> {
        let get = |profile| {
            self.inner
                .providers
                .get(&profile)
                .cloned()
                .ok_or(ProductRunServiceError::ProviderUnavailable)
        };
        Ok(RoleProviders {
            writer: get(selected.writer())?,
            reviewer: get(selected.reviewer())?,
            fixer: get(selected.fixer())?,
            fallbacks: if self.inner.automatic_provider_failover {
                self.inner.providers.values().cloned().collect()
            } else {
                Vec::new()
            },
        })
    }
}
