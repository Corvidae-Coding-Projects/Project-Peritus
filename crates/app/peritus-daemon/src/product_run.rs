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
mod ownership;
mod permissions;
mod persistence;
mod publication;
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
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, RwLock, atomic::AtomicBool},
};

use peritus_app_protocol::{
    AppResponsePayload, ControlOperationId, ProductProviderSelection, ProductRunQuery,
    ProductRunSnapshot, WorkbenchResultPage,
};
use peritus_process::ProcessStore;
use peritus_product_runner::{
    CommandRuntime, ContextSource, PreviewLaunch, ProductRunResume, RoleProviders,
};
use peritus_provider_core::{CancellationToken, ModelProvider};
use peritus_run_settlement::{CandidateCheckpoint, RunSettlement};
use peritus_types::{ProviderProfileId, RunId, Sha256Digest, WorkspaceId};
use tokio::{sync::Mutex, task::JoinHandle};

use crate::{DaemonComponents, DaemonError, startup::workspace::WorkspaceCatalog};

pub use error::{GoverningStateUnavailable, ProductRunServiceError};
use error::{filesystem, invalid};
#[cfg(test)]
use persistence::persist_record;
pub(super) use publication::{
    MutationDisposition, MutationTicket, RunIdentitySnapshot, RunMutationKind,
};
#[cfg(test)]
fn load_records(directory: &Path) -> Result<BTreeMap<RunId, RunRecord>, DaemonError> {
    persistence::load_records(directory)
}
use progress::RunProgress;
use recovery::reconcile_restored_candidates;
use request::ProductRunRequest;
pub(super) use ownership::{PreparedRetry, PreparedRunLaunch};
use snapshot::{
    initial_snapshot, live_snapshot, project_snapshot, replace_snapshot, workspace_has_active_run,
};

#[derive(Clone)]
pub struct ProductRunService {
    inner: Arc<Inner>,
}

struct Inner {
    improvements: std::sync::Mutex<improvements::Store>,
    improvement_launch: Mutex<()>,
    control_generation: crate::product_control::ControlGeneration,
    #[cfg(test)]
    controls: workbench::ControlOwnerQueue,
    control_shutdown: peritus_journal::JournalCancellation,
    control_reconciliation: peritus_journal::JournalCancellation,
    control_store: peritus_journal::StoreId,
    directory: PathBuf,
    publications: publication::RunPublicationManager,
    records: RwLock<BTreeMap<RunId, RunRecord>>,
    run_cancellations: std::sync::Mutex<BTreeMap<RunId, RunCancellation>>,
    providers: BTreeMap<ProviderProfileId, Arc<dyn ModelProvider>>,
    automatic_provider_failover: bool,
    local_context: peritus_product_runner::LocalContextConfig,
    managed_gate_network: peritus_product_runner::ManagedGateNetworkCatalog,
    workspaces: BTreeMap<WorkspaceId, PathBuf>,
    folders: BTreeMap<WorkspaceId, crate::config::FolderDeclaration>,
    processes: ProcessStore,
    tasks: Mutex<ProductRunTasks>,
    model_catalogs: catalog::ModelCatalogs,
    image_decodes: Arc<tokio::sync::Semaphore>,
    host_permissions: permissions::HostPermissionCatalog,
    request_source_artifacts: peritus_artifact_store::StoreConfig,
    finding_bodies: persistence::FindingBodyStore,
    product_artifacts: persistence::ProductArtifactStore,
    request_source_readers: std::sync::Mutex<RequestSourceReaders>,
    reply_artifacts: peritus_artifact_store::StoreConfig,
    reply_readers: std::sync::Mutex<RequestSourceReaders>,
    retained_reply_owners:
        std::sync::Mutex<std::collections::BTreeSet<peritus_product_runner::control::OperationId>>,
    preview_processes: std::sync::Mutex<BTreeMap<ControlOperationId, PreviewProcess>>,
    preview_capture: PreviewCaptureHost,
    #[cfg(test)]
    rewind_faults: std::sync::Mutex<Vec<([u8; 16], workbench::RewindFaultPoint)>>,
}

#[derive(Default)]
struct RequestSourceReaders {
    readers: BTreeMap<
        peritus_types::Sha256Digest,
        Arc<std::sync::Mutex<peritus_artifact_store::ArtifactReadHandle>>,
    >,
    recent: std::collections::VecDeque<peritus_types::Sha256Digest>,
}

#[derive(Clone)]
struct FindingSourceCatalog {
    head_digest: Sha256Digest,
    review_artifacts_externalized: bool,
    entries: Vec<(ContextSource, ReviewArtifactReference)>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RejectedFindingUpdate {
    accepted_head: Sha256Digest,
    rejected_head: Sha256Digest,
    rejected_bytes: u64,
    rejected_finding_state: String,
    phase: u16,
    cycle: u32,
    status: String,
    diff: String,
    gates: String,
    review: String,
    summary: String,
    error: String,
}

#[derive(Clone, Copy)]
struct ReviewArtifactReference {
    digest: Sha256Digest,
    bytes: u64,
    source_ordinal: u64,
}

struct RunCancellation {
    actor: [u8; 16],
    continuation: Option<peritus_product_runner::control::OperationId>,
    attempt_cancelled: Arc<AtomicBool>,
    control: peritus_journal::JournalCancellation,
    provider: CancellationToken,
    reconciliation: Option<Arc<crate::product_control::ControlReconciliation>>,
    user_requested: AtomicBool,
    requested_generation: u64,
    acknowledged_generation: u64,
    cancellation_retainer: bool,
    owner_dropped: bool,
    active: AtomicBool,
    launched: AtomicBool,
    ungoverned: bool,
}

/// Exact immutable control revision owned by one continuation attempt.
///
/// Receipt acceptance lives in C0. This separate run record is installed only when the daemon
/// has retained everything required to recover the same attempt after owner loss.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ContinuationSource {
    operation: peritus_product_runner::control::OperationId,
    revision: u64,
    generation: u64,
    settled: bool,
}

impl ContinuationSource {
    const fn new(
        operation: peritus_product_runner::control::OperationId,
        revision: u64,
        generation: u64,
    ) -> Self {
        Self { operation, revision, generation, settled: false }
    }
}

struct ProductRunTasks {
    accepting: bool,
    owners: Vec<JoinHandle<Result<(), String>>>,
    failed_owners: usize,
    first_owner_failure: Option<String>,
}

impl ProductRunTasks {
    fn new() -> Self {
        Self {
            accepting: true,
            owners: Vec::new(),
            failed_owners: 0,
            first_owner_failure: None,
        }
    }

    fn record_owner_failure(&mut self, detail: impl Into<String>) {
        self.failed_owners = self.failed_owners.saturating_add(1);
        if self.first_owner_failure.is_none() {
            self.first_owner_failure = Some(detail.into());
        }
    }
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
    truncated: std::collections::BTreeSet<ControlOperationId>,
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
    /// Execution attempt generation. Fresh runs start at one and every retry advances it.
    attempt_sequence: u64,
    /// Highest immutable handoff sequence incorporated into this canonical projection.
    handoff_sequence: u64,
    /// Monotonic canonical record mutation revision.
    record_revision: u64,
    /// Immutable acceptance-lineage root at `record_revision`.
    record_lineage_root: Sha256Digest,
    /// Last durable publication head known when this process adopted the record.
    durable_record_revision: u64,
    durable_lineage_root: Sha256Digest,
    durable_canonical_digest: Sha256Digest,
    /// Immutable terminal settlement/reply plan and its monotonic completion evidence.
    settlement_obligation: Option<persistence::SettlementObligation>,
    /// Independent review-artifact migration marker. Zero remains migration-pending.
    review_artifact_migration_version: u16,
    /// Prevents execution while a malformed, gapped, or not-yet-published handoff is unresolved.
    handoff_recovery_pending: bool,
    rejected_finding_update: Option<RejectedFindingUpdate>,
    interaction: interaction::InteractionOptions,
    attempt_admission: Option<peritus_product_runner::control::OperationId>,
    continuation_admissions: Vec<peritus_product_runner::control::OperationId>,
    continuation_sources: Vec<ContinuationSource>,
    request: ProductRunRequest,
    snapshot: ProductRunSnapshot,
    cancelled: Arc<AtomicBool>,
    control_cancellation: peritus_journal::JournalCancellation,
    user_cancelled: bool,
    provider_cancellation: CancellationToken,
    finding_state: String,
    finding_catalog: Arc<FindingSourceCatalog>,
    progress: RunProgress,
    checkpoint: Option<CandidateCheckpoint>,
    settlement: Option<RunSettlement>,
    resume: Option<ProductRunResume>,
    /// An unreadable or newer durable continuation retained byte-for-byte for repair or upgrade.
    opaque_resume: Option<persistence::OpaqueResume>,
    remaining_work: Vec<String>,
    interruption_cause: String,
    candidate_actionable: bool,
    task_baseline_required: bool,
    task_baseline: Option<String>,
    preview: PreviewAggregate,
}

impl RunRecord {
    const fn resume_decode_pending(&self) -> bool {
        self.opaque_resume.is_some()
    }
}

impl ProductRunService {
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
                .map(|record| live_snapshot(&self.inner.directory, record))
                .transpose()
                .map(|snapshot| snapshot.into_iter().collect());
        }
        recent_records(&records, query.offset())
            .into_iter()
            .take(peritus_app_protocol::MAX_PRODUCT_RUN_PAGE)
            .map(|record| live_snapshot(&self.inner.directory, record))
            .collect()
    }

    pub(super) fn project(
        &self,
        snapshot: ProductRunSnapshot,
    ) -> Result<AppResponsePayload, ProductRunServiceError> {
        let records = self.inner.records.read().map_err(|_| ProductRunServiceError::Unavailable)?;
        let record = records.get(&snapshot.run_id()).ok_or(ProductRunServiceError::NotFound)?;
        project_snapshot(&self.inner.directory, record, snapshot)
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
