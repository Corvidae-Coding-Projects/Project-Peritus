//! Daemon-owned product-run registry, persistence, and execution admission.

mod conversation;
mod deliverable;
mod error;
mod execution;
mod lifecycle;
mod persistence;
mod snapshot;

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, RwLock, atomic::AtomicBool},
};

use peritus_app_protocol::{
    ProductConversationMessage, ProductConversationRole, ProductProviderSelection,
    ProductRunContinuation, ProductRunControl, ProductRunControlAction, ProductRunConversation,
    ProductRunConversationQuery, ProductRunPhase, ProductRunQuery, ProductRunRequest,
    ProductRunSnapshot,
};
use peritus_product_runner::RoleProviders;
use peritus_provider_core::{CancellationToken, ModelProvider};
use peritus_types::{ProviderProfileId, RunId, WorkspaceId};
use tokio::{sync::Mutex, task::JoinHandle};

use crate::{DaemonComponents, DaemonError, startup::workspace::WorkspaceCatalog};

use conversation::SharedConversation;
pub use error::ProductRunServiceError;
use error::{filesystem, invalid};
use persistence::{load_records, persist_record};
use snapshot::{initial_snapshot, replace_snapshot, workspace_has_active_run};

#[derive(Clone)]
pub struct ProductRunService {
    inner: Arc<Inner>,
}

struct Inner {
    directory: PathBuf,
    records: RwLock<BTreeMap<RunId, RunRecord>>,
    providers: BTreeMap<ProviderProfileId, Arc<dyn ModelProvider>>,
    workspaces: BTreeMap<WorkspaceId, PathBuf>,
    tasks: Mutex<Vec<JoinHandle<()>>>,
}

struct RunRecord {
    request: ProductRunRequest,
    snapshot: ProductRunSnapshot,
    cancelled: Arc<AtomicBool>,
    provider_cancellation: CancellationToken,
    conversation: Arc<SharedConversation>,
    finding_state: String,
}

impl ProductRunService {
    pub(super) fn open(
        state_root: &Path,
        components: &DaemonComponents,
        workspaces: &WorkspaceCatalog,
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
        let records = load_records(&directory)?;
        Ok(Self {
            inner: Arc::new(Inner {
                directory,
                records: RwLock::new(records),
                providers,
                workspaces: workspaces.roots(),
                tasks: Mutex::new(Vec::new()),
            }),
        })
    }

    pub(super) async fn start(
        &self,
        request: ProductRunRequest,
    ) -> Result<ProductRunSnapshot, ProductRunServiceError> {
        let providers = self.resolve_providers(request.providers())?;
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
                    request: request.clone(),
                    snapshot: snapshot.clone(),
                    cancelled: Arc::clone(&cancelled),
                    provider_cancellation: provider_cancellation.clone(),
                    conversation: Arc::clone(&conversation),
                    finding_state: String::new(),
                },
            );
            persist_record(
                &self.inner.directory,
                records.get(&request.run_id()).expect("inserted product run"),
            )?;
        }
        self.spawn(
            request,
            workspace_root,
            providers,
            cancelled,
            provider_cancellation,
            conversation,
            String::new(),
        )
        .await;
        Ok(snapshot)
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
            return Ok(records
                .get(&run_id)
                .map(|record| vec![record.snapshot.clone()])
                .unwrap_or_default());
        }
        Ok(records
            .values()
            .rev()
            .take(peritus_app_protocol::MAX_PRODUCT_RUNS)
            .map(|record| record.snapshot.clone())
            .collect())
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

    pub(super) async fn continue_run(
        &self,
        continuation: &ProductRunContinuation,
    ) -> Result<ProductRunSnapshot, ProductRunServiceError> {
        let mut restart = None;
        let snapshot = {
            let mut records =
                self.inner.records.write().map_err(|_| ProductRunServiceError::Unavailable)?;
            let workspace_id = records
                .get(&continuation.run_id())
                .ok_or(ProductRunServiceError::NotFound)?
                .request
                .workspace_id();
            let was_terminal = records
                .get(&continuation.run_id())
                .expect("checked product run exists")
                .snapshot
                .phase()
                .terminal();
            if was_terminal
                && workspace_has_active_run(&records, workspace_id, Some(continuation.run_id()))
            {
                return Err(ProductRunServiceError::InvalidState);
            }
            let record =
                records.get_mut(&continuation.run_id()).expect("checked product run exists");
            record
                .conversation
                .append(ProductConversationRole::User, continuation.message().to_owned())?;
            if was_terminal {
                let providers = self.resolve_providers(record.request.providers())?;
                let root = self
                    .inner
                    .workspaces
                    .get(&record.request.workspace_id())
                    .cloned()
                    .ok_or(ProductRunServiceError::WorkspaceUnavailable)?;
                let cancelled = Arc::new(AtomicBool::new(false));
                let token = CancellationToken::new();
                record.cancelled = Arc::clone(&cancelled);
                record.provider_cancellation = token.clone();
                record.snapshot = replace_snapshot(
                    &record.snapshot,
                    ProductRunPhase::Queued,
                    "Follow-up queued for the writer",
                    record.snapshot.summary(),
                )?;
                restart = Some((
                    record.request.clone(),
                    root,
                    providers,
                    cancelled,
                    token,
                    Arc::clone(&record.conversation),
                    record.finding_state.clone(),
                ));
            } else {
                record.snapshot = replace_snapshot(
                    &record.snapshot,
                    record.snapshot.phase(),
                    "Follow-up received; the next model step will incorporate it",
                    record.snapshot.summary(),
                )?;
            }
            persist_record(&self.inner.directory, record)?;
            record.snapshot.clone()
        };
        if let Some((request, root, providers, cancelled, token, conversation, finding_state)) =
            restart
        {
            self.spawn(request, root, providers, cancelled, token, conversation, finding_state)
                .await;
        }
        Ok(snapshot)
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
        })
    }
}
