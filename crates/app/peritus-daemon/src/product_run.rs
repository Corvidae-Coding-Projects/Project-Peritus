//! Daemon-owned product-run registry, persistence, and execution admission.

mod persistence;
mod snapshot;

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use peritus_app_protocol::{
    ProductProviderSelection, ProductRunControl, ProductRunControlAction, ProductRunPhase,
    ProductRunQuery, ProductRunRequest, ProductRunSnapshot,
};
use peritus_product_runner::{ProductRunInput, ProductRunner, RoleProviders, RunObserver};
use peritus_provider_core::{CancellationToken, ModelProvider};
use peritus_types::{ProviderProfileId, RunId, WorkspaceId};
use tokio::{sync::Mutex, task::JoinHandle};

use crate::{
    DaemonComponents, DaemonError, DaemonErrorCode, DaemonRecovery,
    startup::workspace::WorkspaceCatalog,
};

use persistence::{load_records, persist_record};
use snapshot::{initial_snapshot, replace_snapshot};

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
                },
            );
            persist_record(
                &self.inner.directory,
                records.get(&request.run_id()).expect("inserted product run"),
            )?;
        }
        self.spawn(request, workspace_root, providers, cancelled, provider_cancellation).await;
        Ok(snapshot)
    }

    pub(super) async fn control(
        &self,
        control: ProductRunControl,
    ) -> Result<ProductRunSnapshot, ProductRunServiceError> {
        match control.action() {
            ProductRunControlAction::Cancel => self.cancel(control.run_id()),
            ProductRunControlAction::Retry => self.retry(control.run_id()).await,
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

    pub(super) async fn shutdown(&self, timeout: Duration) {
        if let Ok(records) = self.inner.records.read() {
            for record in records.values() {
                record.cancelled.store(true, Ordering::Release);
                let _ = record.provider_cancellation.cancel();
            }
        }
        let mut tasks = self.inner.tasks.lock().await;
        for task in tasks.drain(..) {
            let _ = tokio::time::timeout(timeout, task).await;
        }
    }

    fn cancel(&self, run_id: RunId) -> Result<ProductRunSnapshot, ProductRunServiceError> {
        let mut records =
            self.inner.records.write().map_err(|_| ProductRunServiceError::Unavailable)?;
        let record = records.get_mut(&run_id).ok_or(ProductRunServiceError::NotFound)?;
        if record.snapshot.phase().terminal() {
            return Err(ProductRunServiceError::InvalidState);
        }
        record.cancelled.store(true, Ordering::Release);
        let _ = record.provider_cancellation.cancel();
        record.snapshot = replace_snapshot(
            &record.snapshot,
            record.snapshot.phase(),
            "Cancellation requested",
            record.snapshot.summary(),
        )?;
        persist_record(&self.inner.directory, record)?;
        Ok(record.snapshot.clone())
    }

    async fn retry(&self, run_id: RunId) -> Result<ProductRunSnapshot, ProductRunServiceError> {
        let (request, root, providers, cancelled, token, snapshot) = {
            let mut records =
                self.inner.records.write().map_err(|_| ProductRunServiceError::Unavailable)?;
            let workspace_id = records
                .get(&run_id)
                .ok_or(ProductRunServiceError::NotFound)?
                .request
                .workspace_id();
            if workspace_has_active_run(&records, workspace_id, Some(run_id)) {
                return Err(ProductRunServiceError::InvalidState);
            }
            let record = records.get_mut(&run_id).expect("checked product run exists");
            if !record.snapshot.phase().retryable() {
                return Err(ProductRunServiceError::InvalidState);
            }
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
            record.snapshot = initial_snapshot(&record.request)?;
            persist_record(&self.inner.directory, record)?;
            (record.request.clone(), root, providers, cancelled, token, record.snapshot.clone())
        };
        self.spawn(request, root, providers, cancelled, token).await;
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

    async fn spawn(
        &self,
        request: ProductRunRequest,
        workspace_root: PathBuf,
        providers: RoleProviders,
        cancelled: Arc<AtomicBool>,
        provider_cancellation: CancellationToken,
    ) {
        let service = self.clone();
        let run_id = request.run_id();
        let observer: RunObserver = Arc::new(move |update| service.observe(run_id, update));
        let service = self.clone();
        let task = tokio::spawn(async move {
            let result = ProductRunner::run(
                ProductRunInput {
                    run_id,
                    workspace_root,
                    task: request.task().to_owned(),
                    providers,
                    cancelled,
                    provider_cancellation,
                },
                observer,
            )
            .await;
            service.finish(run_id, result);
        });
        let mut tasks = self.inner.tasks.lock().await;
        tasks.retain(|existing| !existing.is_finished());
        tasks.push(task);
    }

    fn observe(&self, run_id: RunId, update: peritus_product_runner::ProductRunUpdate) {
        let Ok(mut records) = self.inner.records.write() else { return };
        let Some(record) = records.get_mut(&run_id) else { return };
        let phase = match update.phase {
            peritus_product_runner::ProductRunPhase::Writing => ProductRunPhase::Writing,
            peritus_product_runner::ProductRunPhase::Checking => ProductRunPhase::Checking,
            peritus_product_runner::ProductRunPhase::Reviewing => ProductRunPhase::Reviewing,
            peritus_product_runner::ProductRunPhase::Fixing => ProductRunPhase::Fixing,
            peritus_product_runner::ProductRunPhase::Verifying => ProductRunPhase::Verifying,
            peritus_product_runner::ProductRunPhase::Complete => ProductRunPhase::Complete,
        };
        if let Ok(snapshot) = ProductRunSnapshot::new(
            run_id,
            record.request.workspace_id(),
            record.request.providers(),
            phase,
            update.cycle,
            record.request.task().to_owned(),
            update.status,
            update.diff,
            update.gates,
            update.review,
            update.summary,
        ) {
            record.snapshot = snapshot;
            let _ = persist_record(&self.inner.directory, record);
        }
    }

    fn finish(
        &self,
        run_id: RunId,
        result: Result<
            peritus_product_runner::ProductRunOutput,
            peritus_product_runner::ProductRunnerError,
        >,
    ) {
        let Ok(mut records) = self.inner.records.write() else { return };
        let Some(record) = records.get_mut(&run_id) else { return };
        match result {
            Ok(output) => {
                if let Ok(snapshot) = ProductRunSnapshot::new(
                    run_id,
                    record.request.workspace_id(),
                    record.request.providers(),
                    ProductRunPhase::Complete,
                    output.fixer_cycles + 1,
                    record.request.task().to_owned(),
                    "Run completed with passing checks".to_owned(),
                    output.diff,
                    output.gates,
                    output.review,
                    output.summary,
                ) {
                    record.snapshot = snapshot;
                }
            }
            Err(error) => {
                let phase =
                    if error.kind() == peritus_product_runner::ProductRunnerErrorKind::Cancelled {
                        ProductRunPhase::Cancelled
                    } else {
                        ProductRunPhase::Failed
                    };
                if let Ok(snapshot) = replace_snapshot(
                    &record.snapshot,
                    phase,
                    &format!("{} failed", error.operation()),
                    error.detail(),
                ) {
                    record.snapshot = snapshot;
                }
            }
        }
        let _ = persist_record(&self.inner.directory, record);
    }
}

fn workspace_has_active_run(
    records: &BTreeMap<RunId, RunRecord>,
    workspace_id: WorkspaceId,
    except: Option<RunId>,
) -> bool {
    records.iter().any(|(run_id, record)| {
        Some(*run_id) != except
            && record.request.workspace_id() == workspace_id
            && !record.snapshot.phase().terminal()
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProductRunServiceError {
    Duplicate,
    NotFound,
    ProviderUnavailable,
    WorkspaceUnavailable,
    InvalidState,
    InvalidMessage,
    Unavailable,
}

fn filesystem(error: std::io::Error) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Storage,
        DaemonRecovery::Reconcile,
        "access product-run state",
        "product-run state is unavailable",
        error,
    )
}
fn invalid(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::InvalidInput,
        DaemonRecovery::CorrectRequest,
        "configure product runs",
        detail,
    )
}
