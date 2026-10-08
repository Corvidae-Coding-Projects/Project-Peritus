//! Daemon-owned product-run registry, persistence, and execution admission.

mod catalog;
mod construction;
mod deliverable;
mod doctor;
mod error;
mod execution;
mod improvements;
mod interaction;
mod library;
mod lifecycle;
mod operation;
mod permissions;
mod persistence;
mod progress;
mod recovery;
mod request;
mod snapshot;
mod workbench;

#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, RwLock, atomic::AtomicBool},
};

use peritus_app_protocol::{
    AppResponsePayload, ControlOperationId, ProductProviderSelection, ProductRunQuery,
    ProductRunSnapshot, WorkbenchResultPage,
};
use peritus_process::ProcessStore;
use peritus_product_runner::{CommandRuntime, PreviewLaunch, ProductRunResume, RoleProviders};
use peritus_provider_core::{CancellationToken, ModelProvider};
use peritus_run_settlement::{CandidateCheckpoint, RunSettlement};
use peritus_types::{ActionId, ProcessId, ProviderProfileId, RunId, WorkspaceId};
use tokio::{sync::Mutex, task::JoinHandle};

use crate::{DaemonComponents, DaemonError, startup::workspace::WorkspaceCatalog};

pub use error::ProductRunServiceError;
use error::{filesystem, invalid};
use persistence::persist_record;
#[cfg(test)]
fn load_records(directory: &Path) -> Result<BTreeMap<RunId, RunRecord>, DaemonError> {
    persistence::load_records(directory)
}
use progress::RunProgress;
use recovery::reconcile_restored_candidates;
use request::ProductRunRequest;
use snapshot::{
    initial_snapshot, live_snapshot, project_snapshot, replace_snapshot, workspace_has_active_run,
};

#[derive(Clone)]
pub struct ProductRunService {
    inner: Arc<Inner>,
}

type OpenedProductRunService = (ProductRunService, Vec<(RunId, ActionId, ProcessId)>);

struct Inner {
    improvements: std::sync::Mutex<improvements::Store>,
    improvement_launch: Mutex<()>,
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
    tasks: Mutex<Vec<(RunId, JoinHandle<()>)>>,
    command_recoveries: std::sync::Mutex<BTreeSet<RunId>>,
    model_catalogs: catalog::ModelCatalogs,
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
    errors: BTreeMap<ControlOperationId, String>,
    truncated: BTreeSet<ControlOperationId>,
}

#[derive(Clone, Copy)]
struct PreviewOperationRecord {
    fingerprint: peritus_types::Sha256Digest,
    accepted_revision: u64,
    result_sequence: u64,
    completed_sequence: u64,
}

#[derive(Clone)]
struct RunRecord {
    interaction: interaction::InteractionOptions,
    goal_resume: Option<peritus_product_runner::control::OperationId>,
    request: ProductRunRequest,
    snapshot: ProductRunSnapshot,
    cancelled: Arc<AtomicBool>,
    user_cancelled: bool,
    provider_cancellation: CancellationToken,
    finding_state: String,
    progress: RunProgress,
    checkpoint: Option<CandidateCheckpoint>,
    settlement: Option<RunSettlement>,
    resume: Option<ProductRunResume>,
    remaining_work: Vec<String>,
    interruption_cause: String,
    candidate_actionable: bool,
    task_baseline_required: bool,
    task_baseline: Option<String>,
    preview: PreviewAggregate,
}

impl ProductRunService {
    async fn start_configured(
        &self,
        request: ProductRunRequest,
        mut interaction: interaction::InteractionOptions,
    ) -> Result<ProductRunSnapshot, ProductRunServiceError> {
        self.validate_workspace_mode(request.workspace_id(), interaction.mode)?;
        let providers = self.resolve_selected_providers(request.providers(), &interaction)?;
        let workspace_root = self
            .inner
            .workspaces
            .get(&request.workspace_id())
            .cloned()
            .ok_or(ProductRunServiceError::WorkspaceUnavailable)?;
        let snapshot = initial_snapshot(&request)?;
        let cancelled = Arc::new(AtomicBool::new(false));
        let provider_cancellation = CancellationToken::new();
        self.append_control_inputs(&mut interaction)?;
        {
            let mut records =
                self.inner.records.write().map_err(|_| ProductRunServiceError::Unavailable)?;
            if let Some(staged) = records.get(&request.run_id()) {
                let proposed = &interaction.workbench;
                let existing = &staged.interaction.workbench;
                let replace_staged = proposed == existing
                    && staged.snapshot.phase()
                        == peritus_app_protocol::ProductRunPhase::RecoveryRequired
                    && self.with_controls(false, |store| store.resolve(proposed))?.is_none();
                if !replace_staged {
                    return Err(ProductRunServiceError::Duplicate);
                }
            }
            if workspace_has_active_run(&records, request.workspace_id(), None) {
                return Err(ProductRunServiceError::InvalidState);
            }
            deliverable::discard::workspace_available(
                &self.inner.directory,
                &records,
                request.workspace_id(),
            )?;
            records.insert(
                request.run_id(),
                RunRecord {
                    interaction,
                    goal_resume: None,
                    request: request.clone(),
                    snapshot: snapshot.clone(),
                    cancelled: Arc::clone(&cancelled),
                    user_cancelled: false,
                    provider_cancellation: provider_cancellation.clone(),
                    finding_state: String::new(),
                    progress: RunProgress::default(),
                    checkpoint: None,
                    settlement: None,
                    resume: None,
                    remaining_work: Vec::new(),
                    interruption_cause: String::new(),
                    candidate_actionable: false,
                    task_baseline_required: !self
                        .inner
                        .folders
                        .contains_key(&request.workspace_id()),
                    task_baseline: None,
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
            let start = &records
                .get(&request.run_id())
                .expect("inserted product run")
                .interaction
                .workbench;
            if let Err(error) = self.with_controls(false, |store| store.accept(start)) {
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
            String::new(),
            None,
        )
        .await;
        Ok(snapshot)
    }

    fn validate_workspace_mode(
        &self,
        workspace_id: WorkspaceId,
        mode: peritus_app_protocol::ProductInteractionMode,
    ) -> Result<(), ProductRunServiceError> {
        if let Some(folder) = self.inner.folders.get(&workspace_id) {
            folder.verify().map_err(|_| ProductRunServiceError::WorkspaceUnavailable)?;
            if mode == peritus_app_protocol::ProductInteractionMode::Build {
                return Err(ProductRunServiceError::GitRequired);
            }
        }
        Ok(())
    }

    pub(super) fn query(
        &self,
        query: ProductRunQuery,
    ) -> Result<Vec<ProductRunSnapshot>, ProductRunServiceError> {
        let records = self.inner.records.read().map_err(|_| ProductRunServiceError::Unavailable)?;
        if let Some(run_id) = query.run_id() {
            return records
                .get(&run_id)
                .map(|record| live_snapshot(self, record))
                .transpose()
                .map(|snapshot| snapshot.into_iter().collect());
        }
        recent_records(&records, query.offset())
            .into_iter()
            .take(peritus_app_protocol::MAX_PRODUCT_RUN_PAGE)
            .map(|record| live_snapshot(self, record))
            .collect()
    }

    pub(super) fn project(
        &self,
        snapshot: ProductRunSnapshot,
    ) -> Result<AppResponsePayload, ProductRunServiceError> {
        let records = self.inner.records.read().map_err(|_| ProductRunServiceError::Unavailable)?;
        let record = records.get(&snapshot.run_id()).ok_or(ProductRunServiceError::NotFound)?;
        project_snapshot(self, record, snapshot)
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

    pub(super) fn governed_run(&self, run: RunId) -> Result<bool, ProductRunServiceError> {
        let records = self.inner.records.read().map_err(|_| ProductRunServiceError::Unavailable)?;
        Ok(records.contains_key(&run))
    }
}

fn recent_records(records: &BTreeMap<RunId, RunRecord>, offset: u64) -> Vec<&RunRecord> {
    let mut recent = records.values().collect::<Vec<_>>();
    recent.sort_by(|left, right| {
        right
            .progress
            .last_effect_unix_millis
            .cmp(&left.progress.last_effect_unix_millis)
            .then_with(|| {
                right.progress.started_unix_millis.cmp(&left.progress.started_unix_millis)
            })
            .then_with(|| right.snapshot.run_id().cmp(&left.snapshot.run_id()))
    });
    let offset = usize::try_from(offset).unwrap_or(usize::MAX).min(recent.len());
    recent.drain(..offset);
    recent
}
