//! Ordered startup runner for the live ownership bundle.

use std::{fs, path::Path, sync::Arc, time::Duration};

use peritus_app_protocol::AppProtocolLimits;
use peritus_artifact_store::{
    ArtifactCatalogCancellation, ArtifactDigest, ArtifactRepairReason, ArtifactStore,
    ContainedLayoutNamespace, ErrorCode as ArtifactErrorCode, RecoveryObservation,
    RecoverySummary, ReferenceOwnerKind,
};
use peritus_evidence::EvidenceStore;
use peritus_journal::{
    AllocatedAuthorityEpoch, ExpectedAuthorityEpoch, JournalCancellation, NewApplicationPrincipal,
    SqliteJournal, StoreId,
};
use peritus_process::ProcessStore;
use peritus_projection::ProjectionStore;
use peritus_types::Sha256Digest;
use sha2::{Digest, Sha256};
use tokio::sync::{mpsc, watch};

use super::super::{
    evolution::recover_production,
    migration::migrate_existing,
    projection::ensure_current,
    recovery::{reconcile_application, reconcile_processes},
    registry::bootstrap as bootstrap_approval_registry,
    workspace::install_and_reconcile,
};
use super::{DaemonRuntime, progress::StartupProgress};
use crate::instance::InstanceGuard;
use crate::ipc::serve;
use crate::outbox::{DestinationRouter, OutboxRuntime};
use crate::product_run::ProductRunService;
use crate::telemetry::TelemetryRuntime;
use crate::terminal::{TerminalRegistry, TerminalRegistryLimits};
use crate::worker::{WorkerSupervisor, WorkerSupervisorLimits};
use crate::{
    AuthorityOwner, DaemonComponents, DaemonConfig, DaemonError, DaemonErrorCode, DaemonIdentity,
    DaemonLifecycle, DaemonRecovery, LocalEndpoint, StartupPhase, TelemetryExport,
};

struct PreparedStartup {
    config: DaemonConfig,
    store_id: StoreId,
    progress: Option<StartupProgress>,
    identity: DaemonIdentity,
    instance: InstanceGuard,
    journal: SqliteJournal,
    artifacts: ArtifactStore,
    evidence: EvidenceStore,
    processes: ProcessStore,
    components: DaemonComponents,
    terminals: TerminalRegistry,
    workers: WorkerSupervisor,
    projections: ProjectionStore,
    authority_epoch: AllocatedAuthorityEpoch,
    workspaces: super::super::workspace::WorkspaceCatalog,
    production: super::super::evolution::ProductionCatalog,
    product_runs: ProductRunService,
    diagnostic: Option<String>,
    telemetry: Option<TelemetryRuntime>,
}

impl DaemonRuntime {
    /// Validates, locks, migrates, recovers, binds IPC, and enters truthful readiness.
    ///
    /// # Errors
    ///
    /// Returns the exact failed startup boundary. No later worker is started after a failure.
    pub async fn start(config: DaemonConfig) -> Result<Self, DaemonError> {
        match Self::start_cancellable(config, JournalCancellation::new()).await? {
            Some(runtime) => Ok(runtime),
            None => Err(DaemonError::new(
                DaemonErrorCode::Worker,
                DaemonRecovery::Operator,
                "start daemon runtime",
                "startup stopped without cancellation from its caller",
            )),
        }
    }

    /// Starts the daemon while allowing its caller to stop durable ownership waits.
    ///
    /// The blocking startup owners are always joined before this method returns. `None` means the
    /// supplied cancellation won before a complete runtime could be returned.
    ///
    /// # Errors
    ///
    /// Returns the exact non-cancellation startup failure or a blocking-owner join failure.
    pub(crate) async fn start_cancellable(
        config: DaemonConfig,
        cancellation: JournalCancellation,
    ) -> Result<Option<Self>, DaemonError> {
        Self::start_cancellable_with_process_owner(config, cancellation, None).await
    }

    pub(crate) async fn start_cancellable_with_process_owner(
        config: DaemonConfig,
        cancellation: JournalCancellation,
        process_owner: Option<Arc<dyn peritus_process::RetainedProcessTransport>>,
    ) -> Result<Option<Self>, DaemonError> {
        let prepare_cancellation = cancellation.clone();
        let prepared = tokio::task::spawn_blocking(move || {
            prepare_startup(config, &prepare_cancellation, process_owner)
        })
        .await
        .map_err(startup_owner_join_error)??;
        let Some(prepared) = prepared else {
            return Ok(None);
        };
        if cancellation.is_cancelled() {
            return Ok(None);
        }

        let endpoint = LocalEndpoint::bind(
            prepared.config.paths().state_root(),
            &prepared.identity,
        )
        .await?;
        if cancellation.is_cancelled() {
            return Ok(None);
        }

        let finish_cancellation = cancellation.clone();
        let ready = tokio::task::spawn_blocking(move || {
            finish_startup(prepared, endpoint, &finish_cancellation)
        })
        .await
        .map_err(startup_owner_join_error)??;
        let Some((prepared, endpoint, lifecycle)) = ready else {
            return Ok(None);
        };
        if cancellation.is_cancelled() {
            return Ok(None);
        }

        let PreparedStartup {
            config,
            store_id: _,
            progress: _,
            identity,
            instance,
            journal,
            artifacts,
            evidence,
            processes,
            components,
            terminals,
            workers,
            projections,
            authority_epoch,
            workspaces,
            production,
            product_runs,
            diagnostic,
            telemetry,
        } = prepared;
        let read_only = diagnostic.is_some();
        let mut lifecycle = lifecycle;
        if let Some(diagnostic) = diagnostic {
            lifecycle.read_only(diagnostic);
        }
        let (authority, authority_task) = AuthorityOwner::spawn(
            journal,
            lifecycle,
            artifacts,
            config.limits().maximum_artifact_bytes(),
            AppProtocolLimits::PRODUCTION.max_active_idempotency_slots(),
            authority_epoch.get(),
            config.limits().authority_queue(),
        )?;
        let outbox = if read_only {
            None
        } else {
            Some(OutboxRuntime::start(
                authority.clone(),
                DestinationRouter::production_children(&authority, 64)?,
                authority_epoch.get(),
            )?)
        };
        let endpoint_address = endpoint.address().clone();
        let (server_stop, stop) = watch::channel(false);
        let (shutdown_request, shutdown_requests) = mpsc::channel(1);
        let server_task = tokio::spawn(serve(
            endpoint,
            authority.clone(),
            terminals.clone(),
            product_runs.clone(),
            config.limits().maximum_connections(),
            shutdown_request,
            stop,
        ));
        Ok(Some(Self {
            config,
            identity,
            authority,
            authority_task,
            endpoint_address,
            server_stop,
            server_task: Some(server_task),
            shutdown_requests,
            accepted_shutdown: None,
            outbox,
            telemetry,
            workers,
            terminals,
            product_runs,
            _components: components,
            _evidence: evidence,
            processes,
            _projections: projections,
            _production: production,
            _workspaces: workspaces,
            _instance: instance,
        }))
    }
}

fn prepare_startup(
    config: DaemonConfig,
    cancellation: &JournalCancellation,
    process_owner: Option<Arc<dyn peritus_process::RetainedProcessTransport>>,
) -> Result<Option<PreparedStartup>, DaemonError> {
    let artifact_source = cancellation.clone();
    let artifact_cancellation = ArtifactCatalogCancellation::with_cancellation_check(move || {
        artifact_source.is_cancelled()
    });
    let result = artifact_cancellation.run(|| {
        cancellation.run(|| prepare_startup_scoped(config, cancellation, process_owner))
    });
    match result {
        Err(error) if is_cancelled_contention(&error, cancellation) => Ok(None),
        result => result,
    }
}

fn prepare_startup_scoped(
    config: DaemonConfig,
    cancellation: &JournalCancellation,
    process_owner: Option<Arc<dyn peritus_process::RetainedProcessTransport>>,
) -> Result<Option<PreparedStartup>, DaemonError> {
    if cancellation.is_cancelled() {
        return Ok(None);
    }
    let store_id = config.store_identity()?;
    let mut progress = StartupProgress::new(store_id);
    let identity = DaemonIdentity::new(store_id);
    prepare_roots(&config)?;
    progress.complete(StartupPhase::Validate)?;
    let instance = InstanceGuard::acquire(config.paths().state_root(), &identity)?;
    LocalEndpoint::recover_stale(config.paths().state_root(), &identity)?;
    progress.complete(StartupPhase::Lock)?;
    if cancellation.is_cancelled() {
        return Ok(None);
    }
    let database = config.paths().database();
    let fresh_database = !database.exists();
    if !fresh_database {
        migrate_existing(&config, &database)?;
    }
    progress.complete(StartupPhase::Migrate)?;
    if cancellation.is_cancelled() {
        return Ok(None);
    }
    let journal_options = config.limits().journal_options();
    let mut journal = SqliteJournal::open_waiting_with_options(
        &database,
        store_id,
        journal_options,
        cancellation,
    )
    .map_err(storage_error)?;
    if fresh_database {
        drop(journal);
        if cancellation.is_cancelled() {
            return Ok(None);
        }
        migrate_existing(&config, &database)?;
        journal = SqliteJournal::open_waiting_with_options(
            &database,
            store_id,
            journal_options,
            cancellation,
        )
        .map_err(storage_error)?;
    }
    progress.complete(StartupPhase::Journal)?;
    let artifact_config = config
        .limits()
        .artifact_store_config(config.paths().artifact_root())
        .and_then(|value| value.with_database_path(&database))
        .map_err(|error| component_error("open artifact store", error))?;
    let (artifacts, artifact_recovery) = ArtifactStore::open_with_recovery_streaming(
        artifact_config.clone(),
        report_artifact_recovery_observation,
    )
    .map_err(|error| component_error("open artifact store", error))?;
    report_artifact_recovery(&artifacts, artifact_recovery);
    progress.complete(StartupPhase::Artifacts)?;
    let evidence = EvidenceStore::open_waiting(&database, cancellation)
        .map_err(|error| component_error("open evidence store", error))?;
    progress.complete(StartupPhase::Evidence)?;
    let processes = open_process_store(&config, process_owner)?;
    let components = DaemonComponents::build(&config)?;
    let terminals = TerminalRegistry::new(TerminalRegistryLimits::PRODUCTION)
        .map_err(|error| component_error("construct terminal registry", error))?;
    let workers = WorkerSupervisor::new(
        WorkerSupervisorLimits::for_active_tasks(
            config.limits().maximum_workers(),
            Duration::from_millis(config.limits().shutdown_millis()),
        )
        .map_err(|error| component_error("construct worker supervisor", error))?,
    );

    let projections = ensure_current(&mut journal, &database, cancellation)?;
    progress.complete(StartupPhase::Projections)?;
    bootstrap_approval_registry(&mut journal, config.approval_registry())?;
    let expected = journal
        .current_authority_epoch()
        .map_err(storage_error)?
        .map_or(ExpectedAuthorityEpoch::Absent, |current| {
            ExpectedAuthorityEpoch::Current(current.epoch())
        });
    let authority_epoch = journal.allocate_authority_epoch(expected).map_err(storage_error)?;
    progress.complete(StartupPhase::AuthorityEpoch)?;
    let workspaces = install_and_reconcile(&mut journal, &config)?;
    let production = recover_production(&journal, &config, &workspaces)?;
    let product_runs = ProductRunService::open_cancellable(
        config.paths().state_root(),
        store_id,
        &components,
        &workspaces,
        config.product(),
        config.context().local().clone(),
        processes.clone(),
        artifact_config,
        cancellation,
    )?;
    let Some(product_runs) = product_runs else {
        return Ok(None);
    };
    progress.complete(StartupPhase::DomainRecovery)?;
    let diagnostic = reconcile_processes(&processes)?;
    progress.complete(StartupPhase::EffectRecovery)?;
    reconcile_application(&mut journal)?;
    progress.complete(StartupPhase::AppRecovery)?;
    let telemetry = match config.telemetry() {
        TelemetryExport::Disabled => None,
        TelemetryExport::LocalFile { directory, quota_bytes } => {
            Some(TelemetryRuntime::open(&mut journal, store_id, directory, *quota_bytes)?)
        }
    };
    progress.complete(StartupPhase::Outbox)?;
    if cancellation.is_cancelled() {
        return Ok(None);
    }
    Ok(Some(PreparedStartup {
        config,
        store_id,
        progress: Some(progress),
        identity,
        instance,
        journal,
        artifacts,
        evidence,
        processes,
        components,
        terminals,
        workers,
        projections,
        authority_epoch,
        workspaces,
        production,
        product_runs,
        diagnostic,
        telemetry,
    }))
}

fn finish_startup(
    prepared: PreparedStartup,
    endpoint: LocalEndpoint,
    cancellation: &JournalCancellation,
) -> Result<Option<(PreparedStartup, LocalEndpoint, DaemonLifecycle)>, DaemonError> {
    let result = cancellation.run(|| finish_startup_scoped(prepared, endpoint, cancellation));
    match result {
        Err(error) if is_cancelled_contention(&error, cancellation) => Ok(None),
        result => result,
    }
}

fn finish_startup_scoped(
    mut prepared: PreparedStartup,
    endpoint: LocalEndpoint,
    cancellation: &JournalCancellation,
) -> Result<Option<(PreparedStartup, LocalEndpoint, DaemonLifecycle)>, DaemonError> {
    install_local_principal(
        &mut prepared.journal,
        &prepared.config,
        &endpoint,
        prepared.store_id,
    )?;
    if cancellation.is_cancelled() {
        return Ok(None);
    }
    let progress = prepared.progress.as_mut().ok_or_else(|| {
        DaemonError::new(
            DaemonErrorCode::CorruptState,
            DaemonRecovery::Operator,
            "finish daemon startup",
            "startup progress owner is missing",
        )
    })?;
    progress.complete(StartupPhase::Ipc)?;
    progress.complete(StartupPhase::Ready)?;
    let lifecycle = prepared
        .progress
        .take()
        .ok_or_else(|| {
            DaemonError::new(
                DaemonErrorCode::CorruptState,
                DaemonRecovery::Operator,
                "finish daemon startup",
                "startup progress owner is missing",
            )
        })?
        .into_lifecycle()?;
    Ok(Some((prepared, endpoint, lifecycle)))
}

fn is_cancelled_contention(error: &DaemonError, cancellation: &JournalCancellation) -> bool {
    if !cancellation.is_cancelled() {
        return false;
    }
    let mut source = std::error::Error::source(error);
    while let Some(current) = source {
        if current
            .downcast_ref::<peritus_journal::JournalError>()
            .is_some_and(peritus_journal::JournalError::is_contention)
            || current
                .downcast_ref::<peritus_artifact_store::ArtifactStoreError>()
                .is_some_and(|error| error.code() == ArtifactErrorCode::CatalogWaitCancelled)
            || current
                .downcast_ref::<crate::product_control::ControlStoreError>()
                .is_some_and(|error| {
                    matches!(error, crate::product_control::ControlStoreError::ContentionCancelled)
                })
        {
            return true;
        }
        source = current.source();
    }
    false
}

fn startup_owner_join_error(error: tokio::task::JoinError) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Worker,
        DaemonRecovery::Reconcile,
        "join daemon startup owner",
        "blocking daemon startup owner panicked or was cancelled",
        error,
    )
}

fn report_artifact_recovery(store: &ArtifactStore, summary: RecoverySummary) {
    if summary.removed_temporary_files() != 0
        || summary.completed_state_moves() != 0
        || summary.removed_swept_files() != 0
        || summary.reconciled_publications() != 0
    {
        crate::diagnostic::report(&format!(
            "artifact recovery: removed_temporary={} completed_moves={} removed_swept={} reconciled_publications={}",
            summary.removed_temporary_files(),
            summary.completed_state_moves(),
            summary.removed_swept_files(),
            summary.reconciled_publications(),
        ));
    }
    report_repair_obligations(store);
}

fn report_artifact_recovery_observation(observation: RecoveryObservation) {
    match observation {
        RecoveryObservation::ContainedLayout(entry) => {
            let namespace = match entry.namespace() {
                ContainedLayoutNamespace::Temporary => "temporary",
                ContainedLayoutNamespace::Objects => "objects",
                ContainedLayoutNamespace::Quarantine => "quarantine",
            };
            if let Some(digest) = entry.repair_digest() {
                crate::diagnostic::report(&format!(
                    "artifact recovery linked exact containment: identity={:016x} namespace={namespace} digest={}",
                    entry.identity(),
                    digest.to_hex(),
                ));
            } else {
                crate::diagnostic::report(&format!(
                    "artifact recovery contained malformed layout: identity={:016x} namespace={namespace}",
                    entry.identity(),
                ));
            }
        }
        RecoveryObservation::QuarantinedOrphan(orphan) => {
            crate::diagnostic::report(&format!(
                "artifact recovery quarantined uncataloged content: digest={} size={}",
                orphan.digest().to_hex(),
                orphan.size(),
            ));
        }
        RecoveryObservation::ContainedCorruption(corruption) => {
            crate::diagnostic::report(&format!(
                "artifact recovery contained exact content: digest={} size={} reason={} referenced={}",
                corruption.digest().to_hex(),
                corruption.expected_size(),
                repair_reason(corruption.reason()),
                corruption.is_referenced(),
            ));
        }
    }
}

fn report_repair_obligations(store: &ArtifactStore) {
    let mut after = None;
    loop {
        let obligations = match store.repair_obligations_after(after, 256) {
            Ok(obligations) => obligations,
            Err(error) => {
                crate::diagnostic::report(&format!(
                    "artifact repair-obligation query failed: {error}"
                ));
                return;
            }
        };
        if obligations.is_empty() {
            return;
        }
        for obligation in &obligations {
            report_repair_owners(store, *obligation);
            after = Some(obligation.digest());
        }
        if obligations.len() < 256 {
            return;
        }
    }
}

fn report_repair_owners(
    store: &ArtifactStore,
    obligation: peritus_artifact_store::ArtifactRepairObligation,
) {
    let digest = obligation.digest();
    let mut after = None;
    let mut observed_owner = false;
    loop {
        let owners = match store.repair_owners_after(digest, after, 256) {
            Ok(owners) => owners,
            Err(error) => {
                crate::diagnostic::report(&format!(
                    "artifact repair-owner query failed: digest={} size={} reason={} error={error}",
                    digest.to_hex(),
                    obligation.expected_size(),
                    repair_reason(obligation.reason()),
                ));
                return;
            }
        };
        if owners.is_empty() {
            if !observed_owner {
                crate::diagnostic::report(&format!(
                    "artifact repair obligation: digest={} size={} reason={} owner=none",
                    digest.to_hex(),
                    obligation.expected_size(),
                    repair_reason(obligation.reason()),
                ));
            }
            return;
        }
        for owner in &owners {
            observed_owner = true;
            let kind = match owner.kind() {
                ReferenceOwnerKind::Journal => "journal",
                ReferenceOwnerKind::Evidence => "evidence",
            };
            crate::diagnostic::report(&format!(
                "artifact repair obligation: digest={} size={} reason={} owner_kind={kind} owner_identity={}",
                digest.to_hex(),
                obligation.expected_size(),
                repair_reason(obligation.reason()),
                ArtifactDigest::from_sha256(owner.identity()).to_hex(),
            ));
            after = Some(*owner);
        }
        if owners.len() < 256 {
            return;
        }
    }
}

const fn repair_reason(reason: ArtifactRepairReason) -> &'static str {
    match reason {
        ArtifactRepairReason::Missing => "missing",
        ArtifactRepairReason::Corrupt => "corrupt",
    }
}

fn open_process_store(
    config: &DaemonConfig,
    process_owner: Option<Arc<dyn peritus_process::RetainedProcessTransport>>,
) -> Result<ProcessStore, DaemonError> {
    #[cfg(target_os = "linux")]
    let store = if let Some(watchdog) = config.process_crash_watchdog() {
        ProcessStore::open_with_crash_watchdog(
            config.paths().process_root(),
            config.paths().workspace_root(),
            watchdog,
        )
    } else {
        ProcessStore::open(config.paths().process_root(), config.paths().workspace_root())
    }
    .map_err(|error| component_error("open process registry", error))?;
    #[cfg(not(target_os = "linux"))]
    let store = ProcessStore::open(config.paths().process_root(), config.paths().workspace_root())
        .map_err(|error| component_error("open process registry", error))?;
    match process_owner {
        Some(owner) => store
            .with_retained_owner(owner)
            .map_err(|error| component_error("attach retained process owner", error)),
        None => Ok(store),
    }
}

fn install_local_principal(
    journal: &mut SqliteJournal,
    config: &DaemonConfig,
    endpoint: &LocalEndpoint,
    store_id: peritus_journal::StoreId,
) -> Result<(), DaemonError> {
    let peer = endpoint.owner_peer();
    let actor = config.human().actor_identity()?;
    let mut hasher = Sha256::new();
    hasher.update(b"peritus/local-principal-binding/v1\0");
    hasher.update(store_id.as_bytes());
    hasher.update(peer.principal_digest().as_bytes());
    hasher.update(actor.as_bytes());
    let binding = Sha256Digest::new(hasher.finalize().into());
    journal
        .bind_application_principal(NewApplicationPrincipal::new(
            peer.principal_digest(),
            peer.kind(),
            actor,
            binding,
        ))
        .map(|_| ())
        .map_err(storage_error)
}

fn prepare_roots(config: &DaemonConfig) -> Result<(), DaemonError> {
    for path in [
        config.paths().state_root(),
        config.paths().artifact_root(),
        config.paths().evidence_root(),
        config.paths().workspace_root(),
        config.paths().process_root(),
        config.paths().transaction_root(),
        config.paths().backup_root(),
    ] {
        fs::create_dir_all(path).map_err(|error| filesystem_error("create daemon root", error))?;
        let metadata = fs::symlink_metadata(path)
            .map_err(|error| filesystem_error("inspect daemon root", error))?;
        if !metadata.file_type().is_dir() {
            return Err(DaemonError::new(
                DaemonErrorCode::InvalidInput,
                DaemonRecovery::CorrectRequest,
                "validate daemon root",
                "configured daemon root is not a directory",
            ));
        }
        protect_directory(path)?;
        let canonical = fs::canonicalize(path)
            .map_err(|error| filesystem_error("canonicalize daemon root", error))?;
        if canonical != path {
            return Err(DaemonError::new(
                DaemonErrorCode::InvalidInput,
                DaemonRecovery::CorrectRequest,
                "validate daemon root",
                "configured daemon root contains an alias or symlink component",
            ));
        }
        verify_directory_owner(path)?;
    }
    Ok(())
}

#[cfg(unix)]
fn protect_directory(path: &Path) -> Result<(), DaemonError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| filesystem_error("protect daemon root", error))
}

#[cfg(windows)]
const fn protect_directory(_path: &Path) -> Result<(), DaemonError> {
    Ok(())
}

#[cfg(unix)]
fn verify_directory_owner(path: &Path) -> Result<(), DaemonError> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| filesystem_error("inspect protected daemon root", error))?;
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err(DaemonError::new(
            DaemonErrorCode::Unauthorized,
            DaemonRecovery::Operator,
            "validate daemon root ownership",
            "daemon root permissions permit access outside the current operating-system user",
        ));
    }
    Ok(())
}

#[cfg(windows)]
const fn verify_directory_owner(_path: &Path) -> Result<(), DaemonError> {
    Ok(())
}

fn storage_error(error: peritus_journal::JournalError) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Storage,
        DaemonRecovery::Reconcile,
        error.operation(),
        error.to_string(),
        error,
    )
}

fn component_error(
    operation: &'static str,
    error: impl std::error::Error + Send + Sync + 'static,
) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::RecoveryRequired,
        DaemonRecovery::Reconcile,
        operation,
        error.to_string(),
        error,
    )
}

fn filesystem_error(operation: &'static str, error: std::io::Error) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Storage,
        DaemonRecovery::Retry,
        operation,
        "daemon filesystem preparation failed",
        error,
    )
}
