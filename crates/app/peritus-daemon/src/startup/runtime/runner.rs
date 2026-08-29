//! Ordered startup runner for the live ownership bundle.

use std::{fs, path::Path, time::Duration};

use peritus_app_protocol::AppProtocolLimits;
use peritus_artifact_store::{ArtifactStore, StoreConfig};
use peritus_evidence::{EvidenceStore, EvidenceStoreOptions};
use peritus_journal::{
    ExpectedAuthorityEpoch, NewApplicationPrincipal, SqliteJournal, SqliteJournalOptions,
};
use peritus_process::ProcessStore;
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
    DaemonRecovery, LocalEndpoint, StartupPhase, TelemetryExport,
};

impl DaemonRuntime {
    /// Validates, locks, migrates, recovers, binds IPC, and enters truthful readiness.
    ///
    /// # Errors
    ///
    /// Returns the exact failed startup boundary. No later worker is started after a failure.
    pub async fn start(config: DaemonConfig) -> Result<Self, DaemonError> {
        let store_id = config.store_identity()?;
        let mut progress = StartupProgress::new(store_id);
        let identity = DaemonIdentity::new(store_id);
        prepare_roots(&config)?;
        progress.complete(StartupPhase::Validate)?;
        let instance = InstanceGuard::acquire(config.paths().state_root(), &identity)?;
        LocalEndpoint::recover_stale(config.paths().state_root(), &identity)?;
        progress.complete(StartupPhase::Lock)?;
        let database = config.paths().database();
        let fresh_database = !database.exists();
        if !fresh_database {
            migrate_existing(&config, &database)?;
        }
        progress.complete(StartupPhase::Migrate)?;
        let mut journal = SqliteJournal::open(&database, store_id, SqliteJournalOptions::default())
            .map_err(storage_error)?;
        if fresh_database {
            drop(journal);
            migrate_existing(&config, &database)?;
            journal = SqliteJournal::open(&database, store_id, SqliteJournalOptions::default())
                .map_err(storage_error)?;
        }
        progress.complete(StartupPhase::Journal)?;
        let artifact_config = StoreConfig::new(
            config.paths().artifact_root(),
            config.limits().maximum_artifact_bytes(),
            config.limits().artifact_quota_bytes(),
        )
        .and_then(|value| value.with_database_path(&database))
        .map_err(|error| component_error("open artifact store", error))?;
        let artifacts = ArtifactStore::open(artifact_config)
            .map_err(|error| component_error("open artifact store", error))?;
        progress.complete(StartupPhase::Artifacts)?;
        let evidence = EvidenceStore::open(&database, EvidenceStoreOptions::default())
            .map_err(|error| component_error("open evidence store", error))?;
        progress.complete(StartupPhase::Evidence)?;
        let processes =
            ProcessStore::open(config.paths().process_root(), config.paths().workspace_root())
                .map_err(|error| component_error("open process registry", error))?;
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

        let projections = ensure_current(&mut journal, &database)?;
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
        let product_runs = ProductRunService::open(
            config.paths().state_root(),
            &components,
            &workspaces,
            config.product().automatic_provider_failover(),
        )?;
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
        let endpoint = LocalEndpoint::bind(config.paths().state_root(), &identity).await?;
        install_local_principal(&mut journal, &config, &endpoint, store_id)?;
        progress.complete(StartupPhase::Ipc)?;
        progress.complete(StartupPhase::Ready)?;
        product_runs.resume_interrupted().await;
        let mut lifecycle = progress.into_lifecycle()?;
        let read_only = diagnostic.is_some();
        if let Some(diagnostic) = diagnostic {
            lifecycle.read_only(diagnostic);
        }
        let (authority, authority_task) = AuthorityOwner::spawn(
            journal,
            lifecycle,
            artifacts,
            config.limits().maximum_artifact_bytes(),
            AppProtocolLimits::PRODUCTION.max_idempotency_entries(),
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
        Ok(Self {
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
        })
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
