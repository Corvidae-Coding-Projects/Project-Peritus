//! Daemon-owned product-run registry, persistence, and execution admission.

mod catalog;
mod continuation;
mod conversation;
mod deliverable;
mod error;
mod execution;
mod interaction;
mod lifecycle;
mod persistence;
mod progress;
mod recovery;
mod snapshot;

#[cfg(test)]
mod tests;

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, RwLock, atomic::AtomicBool},
};

use peritus_app_protocol::{
    AppResponsePayload, ProductConversationMessage, ProductConversationRole,
    ProductProviderSelection, ProductRunControl, ProductRunControlAction, ProductRunConversation,
    ProductRunConversationQuery, ProductRunQuery, ProductRunRequest, ProductRunSnapshot,
};
use peritus_process::ProcessStore;
use peritus_product_runner::{ProductRunResume, RoleProviders};
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
}

struct RunRecord {
    interaction: Option<interaction::InteractionOptions>,
    request: ProductRunRequest,
    snapshot: ProductRunSnapshot,
    cancelled: Arc<AtomicBool>,
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
}

impl ProductRunService {
    pub(super) fn open(
        state_root: &Path,
        components: &DaemonComponents,
        workspaces: &WorkspaceCatalog,
        automatic_provider_failover: bool,
        local_context: peritus_product_runner::LocalContextConfig,
        processes: ProcessStore,
    ) -> Result<Self, DaemonError> {
        let directory = state_root.join("product-runs");
        fs::create_dir_all(&directory).map_err(filesystem)?;
        let mut providers = BTreeMap::new();
        for key in components.providers().keys() {
            if providers.contains_key(&key.profile_id()) {
                return Err(invalid("product provider identity has multiple configured revisions"));
            }
            let provider = components
                .providers()
                .provider(key.profile_id(), key.revision())
                .ok_or_else(|| invalid("configured product provider could not be resolved"))?;
            providers.insert(key.profile_id(), provider);
        }
        let mut records = load_records(&directory)?;
        let workspace_roots = workspaces.roots();
        reconcile_restored_candidates(&directory, &mut records, &workspace_roots)?;
        Ok(Self {
            inner: Arc::new(Inner {
                directory,
                records: RwLock::new(records),
                providers,
                automatic_provider_failover,
                local_context,
                workspaces: workspace_roots,
                folders: workspaces.folders().clone(),
                processes,
                tasks: Mutex::new(Vec::new()),
                model_catalogs: Mutex::new(BTreeMap::new()),
            }),
        })
    }

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
            if records.contains_key(&request.run_id()) {
                return Err(ProductRunServiceError::Duplicate);
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
                },
            );
            if let Err(error) = persist_record(
                &self.inner.directory,
                records.get(&request.run_id()).expect("inserted product run"),
            ) {
                records.remove(&request.run_id());
                return Err(error);
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
