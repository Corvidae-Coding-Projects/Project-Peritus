//! End-to-end startup owner and orderly runtime teardown.

use std::{fs, path::Path, time::Duration};

use peritus_app_protocol::{AppProtocolLimits, ShutdownRequest};
use peritus_artifact_store::{ArtifactStore, StoreConfig};
use peritus_evidence::{EvidenceStore, EvidenceStoreOptions};
use peritus_journal::{
    ExpectedAuthorityEpoch, NewApplicationPrincipal, SqliteJournal, SqliteJournalOptions,
};
use peritus_process::ProcessStore;
use peritus_projection::ProjectionStore;
use peritus_types::Sha256Digest;
use sha2::{Digest, Sha256};
use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
};

use super::{
    migration::migrate_existing,
    projection::ensure_current,
    recovery::{reconcile_application, reconcile_processes},
    workspace::{WorkspaceCatalog, install_and_reconcile},
};
use crate::instance::InstanceGuard;
use crate::ipc::serve;
use crate::outbox::{DestinationRouter, OutboxRuntime};
use crate::shutdown::{
    ShutdownBounds, ShutdownCoordinator, ShutdownOutcome, ShutdownStage, ShutdownWorkCounts,
};
use crate::telemetry::TelemetryRuntime;
use crate::terminal::{TerminalRegistry, TerminalRegistryLimits};
use crate::worker::{WorkerShutdownDisposition, WorkerSupervisor, WorkerSupervisorLimits};
use crate::{
    AuthorityHandle, AuthorityOwner, DaemonComponents, DaemonConfig, DaemonError, DaemonErrorCode,
    DaemonIdentity, DaemonLifecycle, DaemonRecovery, LocalEndpoint, LocalEndpointAddress,
    StartupPhase, TelemetryExport,
};

/// Live production daemon ownership bundle.
pub struct DaemonRuntime {
    config: DaemonConfig,
    identity: DaemonIdentity,
    authority: AuthorityHandle,
    authority_task: JoinHandle<Result<(), DaemonError>>,
    endpoint_address: LocalEndpointAddress,
    server_stop: watch::Sender<bool>,
    server_task: Option<JoinHandle<Result<(), DaemonError>>>,
    shutdown_requests: mpsc::Receiver<ShutdownRequest>,
    accepted_shutdown: Option<ShutdownRequest>,
    outbox: Option<OutboxRuntime>,
    telemetry: Option<TelemetryRuntime>,
    workers: WorkerSupervisor,
    terminals: TerminalRegistry,
    _components: DaemonComponents,
    _evidence: EvidenceStore,
    processes: ProcessStore,
    _projections: ProjectionStore,
    _workspaces: WorkspaceCatalog,
    _instance: InstanceGuard,
}

impl DaemonRuntime {
    /// Validates, locks, migrates, recovers, binds IPC, and enters truthful readiness.
    ///
    /// # Errors
    ///
    /// Returns the exact failed startup boundary. No later worker is started after a failure.
    pub async fn start(config: DaemonConfig) -> Result<Self, DaemonError> {
        let store_id = config.store_identity()?;
        let identity = DaemonIdentity::new(store_id);
        prepare_roots(&config)?;
        let instance = InstanceGuard::acquire(config.paths().state_root(), &identity)?;
        let database = config.paths().database();
        if database.exists() {
            migrate_existing(&config, &database)?;
        }
        let mut journal = SqliteJournal::open(&database, store_id, SqliteJournalOptions::default())
            .map_err(storage_error)?;
        let artifact_config = StoreConfig::new(
            config.paths().artifact_root(),
            config.limits().maximum_artifact_bytes(),
            config.limits().artifact_quota_bytes(),
        )
        .and_then(|config| config.with_database_path(&database))
        .map_err(|error| component_error("open artifact store", error))?;
        let artifacts = ArtifactStore::open(artifact_config)
            .map_err(|error| component_error("open artifact store", error))?;
        let evidence = EvidenceStore::open(&database, EvidenceStoreOptions::default())
            .map_err(|error| component_error("open evidence store", error))?;
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

        let mut lifecycle = DaemonLifecycle::starting();
        for phase in [
            StartupPhase::Lock,
            StartupPhase::Migrate,
            StartupPhase::Journal,
            StartupPhase::Artifacts,
            StartupPhase::Evidence,
        ] {
            lifecycle.advance(phase)?;
        }
        let projections = ensure_current(&mut journal, &database)?;
        lifecycle.advance(StartupPhase::Projections)?;
        let expected = journal
            .current_authority_epoch()
            .map_err(storage_error)?
            .map_or(ExpectedAuthorityEpoch::Absent, |current| {
                ExpectedAuthorityEpoch::Current(current.epoch())
            });
        let authority_epoch = journal.allocate_authority_epoch(expected).map_err(storage_error)?;
        lifecycle.advance(StartupPhase::AuthorityEpoch)?;
        lifecycle.advance(StartupPhase::DomainRecovery)?;
        let workspaces = install_and_reconcile(&mut journal, &config)?;
        let _workspace_count = workspaces.len();
        let diagnostic = reconcile_processes(&processes)?;
        lifecycle.advance(StartupPhase::EffectRecovery)?;
        reconcile_application(&mut journal)?;
        lifecycle.advance(StartupPhase::AppRecovery)?;
        let telemetry = match config.telemetry() {
            TelemetryExport::Disabled => None,
            TelemetryExport::LocalFile { directory, quota_bytes } => {
                Some(TelemetryRuntime::open(&mut journal, store_id, directory, *quota_bytes)?)
            }
        };
        lifecycle.advance(StartupPhase::Outbox)?;
        let endpoint = LocalEndpoint::bind(config.paths().state_root(), &identity).await?;
        install_local_principal(&mut journal, &config, &endpoint, store_id)?;
        lifecycle.advance(StartupPhase::Ipc)?;
        lifecycle.advance(StartupPhase::Ready)?;
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
            config.limits().authority_queue(),
        )?;
        let outbox = if !read_only {
            Some(OutboxRuntime::start(
                authority.clone(),
                DestinationRouter::empty(64)?,
                authority_epoch.get(),
            )?)
        } else {
            None
        };
        let endpoint_address = endpoint.address().clone();
        let (server_stop, stop) = watch::channel(false);
        let (shutdown_request, shutdown_requests) = mpsc::channel(1);
        let server_task = tokio::spawn(serve(
            endpoint,
            authority.clone(),
            terminals.clone(),
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
            _components: components,
            _evidence: evidence,
            processes,
            _projections: projections,
            _workspaces: workspaces,
            _instance: instance,
        })
    }

    /// Returns stable daemon/store identity.
    #[must_use]
    pub const fn identity(&self) -> &DaemonIdentity {
        &self.identity
    }
    /// Returns the bounded serialized authority client.
    #[must_use]
    pub const fn authority(&self) -> &AuthorityHandle {
        &self.authority
    }
    /// Returns the stable authenticated local endpoint address.
    #[must_use]
    pub const fn endpoint_address(&self) -> &LocalEndpointAddress {
        &self.endpoint_address
    }
    /// Returns the exact accepted A3 shutdown request, when shutdown originated from a client.
    #[must_use]
    pub const fn accepted_shutdown_request(&self) -> Option<ShutdownRequest> {
        self.accepted_shutdown
    }

    /// Waits for the process interrupt/termination request.
    ///
    /// # Errors
    ///
    /// Returns a transport error if the operating system signal source fails.
    pub async fn wait_for_shutdown_signal(&mut self) -> Result<(), DaemonError> {
        #[cfg(unix)]
        {
            let mut terminate =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .map_err(|error| signal_error("register termination signal", error))?;
            tokio::select! {
                result = tokio::signal::ctrl_c() => result.map_err(|error| signal_error("wait for interrupt signal", error)),
                _ = terminate.recv() => Ok(()),
                request = self.shutdown_requests.recv() => {
                    self.accepted_shutdown = request;
                    Ok(())
                },
                result = self.server_task.as_mut().expect("live runtime owns server task") => {
                    self.server_task = None;
                    server_exit(result)
                }
            }
        }
        #[cfg(windows)]
        {
            tokio::select! {
                result = tokio::signal::ctrl_c() => result.map_err(|error| signal_error("wait for interrupt signal", error)),
                request = self.shutdown_requests.recv() => {
                    self.accepted_shutdown = request;
                    Ok(())
                },
                result = self.server_task.as_mut().expect("live runtime owns server task") => {
                    self.server_task = None;
                    server_exit(result)
                }
            }
        }
    }

    /// Closes intake and joins the sole authority task within the configured bound.
    ///
    /// # Errors
    ///
    /// Returns the exact clean/unclean outcome after every cleanup boundary has been attempted.
    pub async fn shutdown(mut self) -> Result<ShutdownOutcome, DaemonError> {
        let mut coordinator = ShutdownCoordinator::begin(
            self.accepted_shutdown,
            ShutdownBounds::from_protocol(AppProtocolLimits::PRODUCTION)?,
        )?;
        let mut indeterminate_effects = 0;
        retain_cleanup_failure(
            &mut coordinator,
            &mut indeterminate_effects,
            self.authority.begin_draining().await,
        )?;
        let _ = self.workers.begin_draining();
        let _ = self.server_stop.send(true);
        coordinator.record_stage(
            ShutdownStage::AdmissionClosed,
            self.shutdown_counts(0, 0, indeterminate_effects),
        )?;
        let timeout = Duration::from_millis(self.config.limits().shutdown_millis());
        if let Some(mut server_task) = self.server_task.take() {
            match tokio::time::timeout(timeout, &mut server_task).await {
                Ok(result) => retain_cleanup_failure(
                    &mut coordinator,
                    &mut indeterminate_effects,
                    server_exit(result),
                )?,
                Err(_) => {
                    server_task.abort();
                    let _ = server_task.await;
                    retain_cleanup_failure(
                        &mut coordinator,
                        &mut indeterminate_effects,
                        Err(DaemonError::new(
                            DaemonErrorCode::UncleanShutdown,
                            DaemonRecovery::Reconcile,
                            "join local endpoint server",
                            "connection tasks exceeded the configured shutdown bound",
                        )),
                    )?;
                }
            }
        }
        coordinator.record_stage(
            ShutdownStage::ConnectionsJoined,
            self.shutdown_counts(0, 0, indeterminate_effects),
        )?;
        if let Some(outbox) = &mut self.outbox {
            retain_cleanup_failure(
                &mut coordinator,
                &mut indeterminate_effects,
                outbox.shutdown(timeout).await,
            )?;
        }
        coordinator.record_stage(
            ShutdownStage::OutboxSettled,
            self.shutdown_counts(0, 0, indeterminate_effects),
        )?;
        let worker_report = self.workers.shutdown().await;
        let worker_remaining = worker_report.remaining().len();
        indeterminate_effects =
            indeterminate_effects.saturating_add(worker_report.observations().len());
        if worker_report.disposition() != WorkerShutdownDisposition::Clean {
            retain_cleanup_failure(
                &mut coordinator,
                &mut indeterminate_effects,
                Err(DaemonError::new(
                    DaemonErrorCode::UncleanShutdown,
                    DaemonRecovery::Reconcile,
                    "join supervised workers",
                    "worker tasks or terminal observations remain unsettled",
                )),
            )?;
        }
        coordinator.record_stage(
            ShutdownStage::WorkersJoined,
            self.shutdown_counts(worker_remaining, 0, indeterminate_effects),
        )?;
        let process_reconciliation = match reconcile_processes(&self.processes) {
            Ok(None) => Ok(()),
            Ok(Some(_)) => Err(DaemonError::new(
                DaemonErrorCode::RecoveryRequired,
                DaemonRecovery::Reconcile,
                "reconcile processes during shutdown",
                "one or more process records remain indeterminate",
            )),
            Err(error) => Err(error),
        };
        retain_cleanup_failure(
            &mut coordinator,
            &mut indeterminate_effects,
            process_reconciliation,
        )?;
        if let Some(telemetry) = &mut self.telemetry {
            retain_cleanup_failure(
                &mut coordinator,
                &mut indeterminate_effects,
                telemetry.shutdown(),
            )?;
        }
        coordinator.record_stage(
            ShutdownStage::ProcessesReconciled,
            self.shutdown_counts(worker_remaining, 0, indeterminate_effects),
        )?;
        retain_cleanup_failure(
            &mut coordinator,
            &mut indeterminate_effects,
            self.authority.stop().await,
        )?;
        let authority_result = match tokio::time::timeout(timeout, &mut self.authority_task).await {
            Ok(Ok(result)) => result,
            Ok(Err(error)) => Err(DaemonError::with_source(
                DaemonErrorCode::UncleanShutdown,
                DaemonRecovery::Reconcile,
                "join authority owner",
                "authority task panicked or was cancelled",
                error,
            )),
            Err(_) => {
                self.authority_task.abort();
                Err(DaemonError::new(
                    DaemonErrorCode::UncleanShutdown,
                    DaemonRecovery::Reconcile,
                    "join authority owner",
                    "authority task exceeded the configured shutdown bound",
                ))
            }
        };
        retain_cleanup_failure(&mut coordinator, &mut indeterminate_effects, authority_result)?;
        let final_counts = self.shutdown_counts(worker_remaining, 0, indeterminate_effects);
        coordinator.record_stage(ShutdownStage::AuthorityStopped, final_counts)?;
        coordinator.complete(final_counts)
    }

    fn shutdown_counts(
        &self,
        workers: usize,
        outbox: usize,
        indeterminate_effects: usize,
    ) -> ShutdownWorkCounts {
        let (_, terminal_attachments) = self.terminals.counts();
        ShutdownWorkCounts::empty()
            .with_terminal_attachments(terminal_attachments)
            .with_workers(workers)
            .with_processes(self.processes.recovery_work_count())
            .with_outbox(outbox)
            .with_indeterminate_effects(indeterminate_effects)
    }
}

fn retain_cleanup_failure(
    coordinator: &mut ShutdownCoordinator,
    indeterminate_effects: &mut usize,
    result: Result<(), DaemonError>,
) -> Result<(), DaemonError> {
    if let Err(error) = result {
        coordinator.record_failure(error.code_kind())?;
        *indeterminate_effects = indeterminate_effects.saturating_add(1);
    }
    Ok(())
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
fn protect_directory(_path: &Path) -> Result<(), DaemonError> {
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
fn verify_directory_owner(_path: &Path) -> Result<(), DaemonError> {
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

fn signal_error(operation: &'static str, error: std::io::Error) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Transport,
        DaemonRecovery::Operator,
        operation,
        "daemon signal source failed",
        error,
    )
}

fn server_exit(
    result: Result<Result<(), DaemonError>, tokio::task::JoinError>,
) -> Result<(), DaemonError> {
    match result {
        Ok(result) => result,
        Err(error) => Err(DaemonError::with_source(
            DaemonErrorCode::Worker,
            DaemonRecovery::Reconcile,
            "join local endpoint server",
            "local endpoint server panicked or was cancelled",
            error,
        )),
    }
}
