//! Packaged sibling-daemon resolution, startup, and bounded readiness.

use std::{
    fs::{self, File, OpenOptions},
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
    process::{ExitStatus, Stdio},
    time::{Duration, Instant},
};

use peritus_app_client::{Client, ClientErrorKind, RequestIdentity};
use peritus_app_protocol::{
    AppRequestPayload, AppResponsePayload, DaemonHealth, DaemonInstance, DaemonReadiness,
    ProductRunQuery, ShutdownRequest, WellKnownProtocolFeature,
};
use peritus_process::{NativeProcessProbe, ProcessProbe, ProcessTreeIdentity};
use peritus_provider_core::{CancellationToken, first as cancel_first};
use peritus_tui::ProductLaunchContext;
use peritus_types::Sha256Digest;
use sha2::{Digest, Sha256};
use tokio::{
    io::{AsyncRead, AsyncReadExt as _},
    process::{Child, Command},
};

use crate::{LauncherError, PreparedProduct, persistence::publish_new};

const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);
const MAX_RETAINED_PROCESS_OUTPUT_BYTES: usize = 64 * 1_024;

/// Exact packaged executables used by product composition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SiblingBinaries {
    application: PathBuf,
    daemon: PathBuf,
}

impl SiblingBinaries {
    /// Resolves and version-checks `peritusd` beside the running `peritus` executable.
    ///
    /// # Errors
    ///
    /// Returns an actionable packaging error if the executable is absent or version-mismatched.
    pub async fn discover() -> Result<Self, LauncherError> {
        Self::discover_cancellable(&CancellationToken::new()).await
    }

    /// Resolves and version-checks packaged siblings with caller-owned cancellation.
    ///
    /// # Errors
    ///
    /// Returns an actionable packaging or cancellation error.
    pub async fn discover_cancellable(
        cancellation: &CancellationToken,
    ) -> Result<Self, LauncherError> {
        let current = std::env::current_exe().map_err(|error| {
            LauncherError::DaemonBinary(format!("cannot locate running executable: {error}"))
        })?;
        let directory = current
            .parent()
            .ok_or_else(|| {
                LauncherError::DaemonBinary("running executable has no parent directory".to_owned())
            })?
            .to_path_buf();
        let name = if cfg!(windows) { "peritusd.exe" } else { "peritusd" };
        Self::from_paths(current, directory.join(name), cancellation).await
    }

    /// Validates one explicit daemon executable path.
    ///
    /// # Errors
    ///
    /// Returns an actionable packaging error for a missing, non-file, or mismatched executable.
    pub async fn from_daemon(daemon: PathBuf) -> Result<Self, LauncherError> {
        Self::from_daemon_cancellable(daemon, &CancellationToken::new()).await
    }

    /// Validates one explicit daemon path with caller-owned cancellation.
    ///
    /// # Errors
    ///
    /// Returns an actionable packaging or cancellation error.
    pub async fn from_daemon_cancellable(
        daemon: PathBuf,
        cancellation: &CancellationToken,
    ) -> Result<Self, LauncherError> {
        let application = std::env::current_exe().map_err(|error| {
            LauncherError::DaemonBinary(format!("cannot locate running executable: {error}"))
        })?;
        Self::from_paths(application, daemon, cancellation).await
    }

    async fn from_paths(
        application: PathBuf,
        daemon: PathBuf,
        cancellation: &CancellationToken,
    ) -> Result<Self, LauncherError> {
        let metadata = tokio::fs::metadata(&daemon).await.map_err(|error| {
            LauncherError::DaemonBinary(format!("{}: {error}", daemon.display()))
        })?;
        if !metadata.is_file() {
            return Err(LauncherError::DaemonBinary(format!(
                "{} is not a regular file",
                daemon.display()
            )));
        }
        let mut command = Command::new(&daemon);
        command.arg("--version");
        let output = bounded_output(command, "check packaged daemon version", cancellation)
            .await
            .map_err(|error| match error {
                LauncherError::Cancelled { .. } => error,
                other => LauncherError::DaemonBinary(format!(
                    "cannot execute {} for version check: {other}",
                    daemon.display(),
                )),
            })?;
        let expected = format!("peritusd {}\n", env!("CARGO_PKG_VERSION"));
        if !output.status.success()
            || output.stdout.truncated
            || output.stdout.total_bytes != u64::try_from(expected.len()).unwrap_or(u64::MAX)
            || output.stdout.bytes != expected.as_bytes()
        {
            return Err(LauncherError::DaemonBinary(format!(
                "{} is not the matching Peritus {} daemon (status {}, stdout bytes {}, stderr bytes {})",
                daemon.display(),
                env!("CARGO_PKG_VERSION"),
                output.status,
                output.stdout.total_bytes,
                output.stderr.total_bytes,
            )));
        }
        Ok(Self { application, daemon })
    }

    /// Borrows the exact daemon executable path.
    #[must_use]
    pub fn daemon(&self) -> &Path {
        &self.daemon
    }

    /// Borrows the exact matching application executable path.
    #[must_use]
    pub fn application(&self) -> &Path {
        &self.application
    }
    /// Returns the packaged version verified when this checked binary pair was constructed.
    /// This is a historical observation, not a fresh executable probe.
    #[must_use]
    pub const fn verified_version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }
}

/// Outcome of establishing live daemon readiness.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DaemonLaunch {
    /// An already-running matching local daemon was reused.
    Reused,
    /// A new packaged daemon process was started.
    Started {
        /// Native process identifier of the launched daemon.
        process_id: u32,
    },
}

/// Outcome of a bounded product-owned daemon shutdown request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DaemonShutdown {
    /// No reachable daemon existed.
    AlreadyStopped,
    /// The daemon accepted shutdown and withdrew its endpoint.
    Stopped,
}

/// Singleton daemon startup and readiness supervisor with caller-selected bounds.
#[derive(Debug)]
pub struct DaemonSupervisor {
    readiness_timeout: Option<Duration>,
    owner: Option<OwnedSupervisor>,
}

#[derive(Debug)]
struct OwnedSupervisor {
    child: Child,
    tree: ProcessTreeIdentity,
}

struct HealthConnection {
    client: Client,
    health: Option<DaemonHealth>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SupervisorCustody {
    generation: u64,
    configuration: PathBuf,
    configuration_digest: Sha256Digest,
}

struct ReconciliationOwner {
    lock: File,
}

impl Drop for ReconciliationOwner {
    fn drop(&mut self) {
        let _ = fs4::FileExt::unlock(&self.lock);
    }
}

impl DaemonSupervisor {
    /// Creates a supervisor with one explicit startup bound.
    #[must_use]
    pub const fn new(readiness_timeout: Duration) -> Self {
        Self { readiness_timeout: Some(readiness_timeout), owner: None }
    }

    /// Creates a supervisor that waits for readiness or process exit without imposing a deadline.
    #[must_use]
    pub const fn without_deadline() -> Self {
        Self { readiness_timeout: None, owner: None }
    }

    /// Reuses a reachable endpoint or starts the packaged daemon and waits for readiness.
    ///
    /// # Errors
    ///
    /// Returns spawn, early-exit, timeout, or diagnostic-log failures.
    pub async fn ensure_ready(
        &mut self,
        product: &PreparedProduct,
        binaries: &SiblingBinaries,
    ) -> Result<DaemonLaunch, LauncherError> {
        self.ensure_ready_cancellable(product, binaries, &CancellationToken::new()).await
    }

    /// Establishes exact authenticated daemon health with caller-owned cancellation.
    ///
    /// # Errors
    ///
    /// Returns spawn, identity, early-exit, timeout, protocol, or cancellation failures.
    pub async fn ensure_ready_cancellable(
        &mut self,
        product: &PreparedProduct,
        binaries: &SiblingBinaries,
        cancellation: &CancellationToken,
    ) -> Result<DaemonLaunch, LauncherError> {
        let started = Instant::now();
        let _reconciliation = self
            .acquire_reconciliation_owner(product, started, cancellation)
            .await?;
        self.ensure_ready_owned(product, None, binaries, started, cancellation).await
    }

    /// Refreshes and reconciles the current product generation while preserving an open UI scope.
    pub(crate) async fn reconcile_ready_cancellable(
        &mut self,
        product: &mut PreparedProduct,
        launch: &ProductLaunchContext,
        binaries: &SiblingBinaries,
        cancellation: &CancellationToken,
    ) -> Result<DaemonLaunch, LauncherError> {
        let started = Instant::now();
        let _reconciliation = self
            .acquire_reconciliation_owner(product, started, cancellation)
            .await?;
        product.refresh_for_reconciliation(launch)?;
        self.ensure_ready_owned(product, Some(launch), binaries, started, cancellation).await
    }

    async fn ensure_ready_owned(
        &mut self,
        product: &PreparedProduct,
        launch: Option<&ProductLaunchContext>,
        binaries: &SiblingBinaries,
        started: Instant,
        cancellation: &CancellationToken,
    ) -> Result<DaemonLaunch, LauncherError> {
        let executable_digest = Sha256Digest::new(
            file_digest_cancellable(binaries.daemon(), cancellation).await?,
        );
        let expected_implementation = format!("peritusd/{}", binaries.verified_version());
        if let Some(mut connection) =
            probe_health(product, self.remaining(started), cancellation).await?
        {
            match classify_health(
                product,
                executable_digest,
                &expected_implementation,
                &connection,
                self.owner.is_some(),
            )? {
                HealthDisposition::Ready { custody, stale } => {
                    stop_supervisors(binaries, &stale, cancellation).await?;
                    record_reconciliation_receipt(product, connection.health.as_ref(), &custody)?;
                    return Ok(DaemonLaunch::Reused);
                }
                HealthDisposition::Replace { active, stale } => {
                    let supervisors = request_replacement(
                        &mut connection.client,
                        launch,
                        binaries,
                        active,
                        stale,
                        cancellation,
                    )
                    .await?;
                    self.wait_for_withdrawal(product, &supervisors, started, cancellation).await?;
                }
                HealthDisposition::Wait { stale } => {
                    stop_supervisors(binaries, &stale, cancellation).await?;
                    if let Some(launch) = self
                        .wait_for_existing(
                            product,
                            launch,
                            binaries,
                            executable_digest,
                            &expected_implementation,
                            started,
                            cancellation,
                        )
                        .await?
                    {
                        return Ok(launch);
                    }
                }
            }
        }

        let compatible_supervisor =
            settle_unready_supervisors(product, binaries, cancellation).await?;
        if (!instance_lock_available(product)?
            || !supervisor_lock_available(product)?
            || compatible_supervisor)
            && let Some(launch) = self
                .wait_for_existing(
                    product,
                    launch,
                    binaries,
                    executable_digest,
                    &expected_implementation,
                    started,
                    cancellation,
                )
                .await?
        {
            return Ok(launch);
        }

        let log_path = product.layout().daemon_log();
        if let Some(status) = self.owned_exit()?
            && !status.success()
        {
            return Err(LauncherError::DaemonExited { status, log: log_path });
        }
        if self.owner.is_none() {
            if cancellation.is_cancelled() {
                return Err(LauncherError::Cancelled { operation: "start daemon supervisor" });
            }
            self.owner = Some(spawn_daemon(binaries, product, &log_path).await?);
        }
        let mut exited_status = None;
        loop {
            if let Some(mut connection) =
                probe_health(product, self.remaining(started), cancellation).await?
            {
                match classify_health(
                    product,
                    executable_digest,
                    &expected_implementation,
                    &connection,
                    self.owner.is_some(),
                )? {
                    HealthDisposition::Ready { custody, stale } => {
                        stop_supervisors(binaries, &stale, cancellation).await?;
                        let process_id = connection
                            .health
                            .as_ref()
                            .map(|health| health.instance().process_id())
                            .ok_or_else(|| {
                                LauncherError::DaemonSpawn(
                                    "legacy daemon was classified as exact healthy state"
                                        .to_owned(),
                                )
                            })?;
                        record_reconciliation_receipt(
                            product,
                            connection.health.as_ref(),
                            &custody,
                        )?;
                        return Ok(DaemonLaunch::Started { process_id });
                    }
                    HealthDisposition::Replace { active, stale } => {
                        let supervisors = request_replacement(
                            &mut connection.client,
                            launch,
                            binaries,
                            active,
                            stale,
                            cancellation,
                        )
                        .await?;
                        self.wait_for_withdrawal(
                            product,
                            &supervisors,
                            started,
                            cancellation,
                        )
                        .await?;
                        return Err(LauncherError::DaemonSpawn(
                            "started daemon reported a different store, configuration, or executable identity"
                                .to_owned(),
                        ));
                    }
                    HealthDisposition::Wait { stale } => {
                        stop_supervisors(binaries, &stale, cancellation).await?;
                    }
                }
            }
            if let Some(status) = self.owned_exit()? {
                exited_status = Some(status);
            }
            if let Some(status) = exited_status
                && supervisor_lock_available(product)?
                && instance_lock_available(product)?
            {
                return Err(LauncherError::DaemonExited { status, log: log_path });
            }
            self.wait_progress(started, &log_path, cancellation).await?;
        }
    }

    /// Requests orderly shutdown through the stable automation surface and waits for withdrawal.
    ///
    /// # Errors
    ///
    /// Returns a bounded process or timeout failure when the reachable daemon does not stop.
    pub async fn shutdown(
        &mut self,
        product: &PreparedProduct,
        _binaries: &SiblingBinaries,
    ) -> Result<DaemonShutdown, LauncherError> {
        self.shutdown_cancellable(product, &CancellationToken::new()).await
    }

    /// Requests authenticated shutdown with caller-owned cancellation.
    ///
    /// # Errors
    ///
    /// Returns identity, protocol, timeout, or cancellation failures.
    pub async fn shutdown_cancellable(
        &mut self,
        product: &PreparedProduct,
        cancellation: &CancellationToken,
    ) -> Result<DaemonShutdown, LauncherError> {
        let started = Instant::now();
        let _reconciliation = self
            .acquire_reconciliation_owner(product, started, cancellation)
            .await?;
        let Some(mut connection) =
            probe_health(product, self.remaining(started), cancellation).await?
        else {
            return if instance_lock_available(product)? {
                Ok(DaemonShutdown::AlreadyStopped)
            } else {
                Err(LauncherError::DaemonSpawn(
                    "daemon instance lock is owned but authenticated health is unavailable"
                        .to_owned(),
                ))
            };
        };
        if let Some(health) = &connection.health {
            require_store(product, health)?;
        }
        request_shutdown(&mut connection.client, cancellation).await?;
        self.wait_for_withdrawal(product, &[], started, cancellation).await?;
        Ok(DaemonShutdown::Stopped)
    }

    async fn wait_for_existing(
        &mut self,
        product: &PreparedProduct,
        launch: Option<&ProductLaunchContext>,
        binaries: &SiblingBinaries,
        executable_digest: Sha256Digest,
        expected_implementation: &str,
        started: Instant,
        cancellation: &CancellationToken,
    ) -> Result<Option<DaemonLaunch>, LauncherError> {
        let log_path = product.layout().daemon_log();
        loop {
            match probe_health(product, self.remaining(started), cancellation).await? {
                Some(mut connection) => match classify_health(
                    product,
                    executable_digest,
                    expected_implementation,
                    &connection,
                    self.owner.is_some(),
                )? {
                    HealthDisposition::Ready { custody, stale } => {
                        stop_supervisors(binaries, &stale, cancellation).await?;
                        record_reconciliation_receipt(
                            product,
                            connection.health.as_ref(),
                            &custody,
                        )?;
                        return Ok(Some(DaemonLaunch::Reused));
                    }
                    HealthDisposition::Replace { active, stale } => {
                        let supervisors = request_replacement(
                            &mut connection.client,
                            launch,
                            binaries,
                            active,
                            stale,
                            cancellation,
                        )
                        .await?;
                        self.wait_for_withdrawal(
                            product,
                            &supervisors,
                            started,
                            cancellation,
                        )
                        .await?;
                        return Ok(None);
                    }
                    HealthDisposition::Wait { stale } => {
                        stop_supervisors(binaries, &stale, cancellation).await?;
                    }
                },
                None
                    if instance_lock_available(product)?
                        && supervisor_lock_available(product)?
                        && !compatible_supervisor_held(product)? =>
                {
                    return Ok(None);
                }
                None => {}
            }
            self.wait_progress(started, &log_path, cancellation).await?;
        }
    }

    async fn wait_for_withdrawal(
        &mut self,
        product: &PreparedProduct,
        supervisors: &[SupervisorCustody],
        started: Instant,
        cancellation: &CancellationToken,
    ) -> Result<(), LauncherError> {
        let log_path = product.layout().daemon_log();
        loop {
            if instance_lock_available(product)?
                && supervisor_lock_available(product)?
                && supervisors_released(supervisors)?
            {
                self.reap_owned()?;
                return Ok(());
            }
            self.wait_progress(started, &log_path, cancellation).await?;
        }
    }

    async fn wait_progress(
        &mut self,
        started: Instant,
        log_path: &Path,
        cancellation: &CancellationToken,
    ) -> Result<(), LauncherError> {
        if let Some(timeout) = self.readiness_timeout
            && started.elapsed() >= timeout
        {
            self.stop_owned_supervisor().await;
            return Err(LauncherError::DaemonTimeout {
                seconds: timeout.as_secs(),
                log: log_path.to_path_buf(),
            });
        }
        if cancel_first(cancellation, tokio::time::sleep(PROGRESS_INTERVAL)).await.is_none() {
            self.stop_owned_supervisor().await;
            return Err(LauncherError::Cancelled { operation: "wait for daemon readiness" });
        }
        Ok(())
    }

    async fn acquire_reconciliation_owner(
        &mut self,
        product: &PreparedProduct,
        started: Instant,
        cancellation: &CancellationToken,
    ) -> Result<ReconciliationOwner, LauncherError> {
        let path = product.layout().daemon_reconciliation_lock();
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|error| {
                LauncherError::filesystem("open daemon reconciliation lock", &path, error)
            })?;
        crate::persistence::protect_file(&lock, &path)?;
        loop {
            match fs4::FileExt::try_lock(&lock) {
                Ok(()) => return Ok(ReconciliationOwner { lock }),
                Err(fs4::TryLockError::WouldBlock) => {
                    self.wait_progress(started, &product.layout().daemon_log(), cancellation)
                        .await?;
                }
                Err(fs4::TryLockError::Error(error)) => {
                    return Err(LauncherError::filesystem(
                        "acquire daemon reconciliation lock",
                        path,
                        error,
                    ));
                }
            }
        }
    }

    fn remaining(&self, started: Instant) -> Option<Duration> {
        self.readiness_timeout.map(|timeout| timeout.saturating_sub(started.elapsed()))
    }

    fn owned_exit(&mut self) -> Result<Option<ExitStatus>, LauncherError> {
        let Some(owner) = self.owner.as_mut() else { return Ok(None) };
        let status = owner
            .child
            .try_wait()
            .map_err(|error| LauncherError::DaemonSpawn(error.to_string()))?;
        if status.is_some() {
            self.owner = None;
        }
        Ok(status)
    }

    fn reap_owned(&mut self) -> Result<(), LauncherError> {
        let _ = self.owned_exit()?;
        Ok(())
    }

    async fn stop_owned_supervisor(&mut self) {
        if let Some(owner) = self.owner.take() {
            stop_supervisor(owner).await;
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum HealthDisposition {
    Ready {
        custody: SupervisorCustody,
        stale: Vec<SupervisorCustody>,
    },
    Wait {
        stale: Vec<SupervisorCustody>,
    },
    Replace {
        active: Option<SupervisorCustody>,
        stale: Vec<SupervisorCustody>,
    },
}

fn instance_lock_available(product: &PreparedProduct) -> Result<bool, LauncherError> {
    let path = product.daemon_config().paths().state_root().join("daemon.lock");
    let file = match OpenOptions::new().read(true).write(true).open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Err(error) => {
            return Err(LauncherError::filesystem("open daemon instance lock", path, error));
        }
    };
    match fs4::FileExt::try_lock(&file) {
        Ok(()) => {
            let _ = fs4::FileExt::unlock(&file);
            Ok(true)
        }
        Err(fs4::TryLockError::WouldBlock) => Ok(false),
        Err(fs4::TryLockError::Error(error)) => {
            Err(LauncherError::filesystem("probe daemon instance lock", path, error))
        }
    }
}

fn supervisor_lock_available(product: &PreparedProduct) -> Result<bool, LauncherError> {
    supervisor_lock_available_at(&product.daemon_config_path())
}

fn supervisor_lock_available_at(configuration: &Path) -> Result<bool, LauncherError> {
    let mut path = configuration.as_os_str().to_owned();
    path.push(".supervisor.lock");
    let path = PathBuf::from(path);
    let file = match OpenOptions::new().read(true).write(true).open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Err(error) => {
            return Err(LauncherError::filesystem(
                "open daemon supervisor lock",
                &path,
                error,
            ));
        }
    };
    match fs4::FileExt::try_lock(&file) {
        Ok(()) => {
            let _ = fs4::FileExt::unlock(&file);
            Ok(true)
        }
        Err(fs4::TryLockError::WouldBlock) => Ok(false),
        Err(fs4::TryLockError::Error(error)) => Err(LauncherError::filesystem(
            "probe daemon supervisor lock",
            path,
            error,
        )),
    }
}

async fn probe_health(
    product: &PreparedProduct,
    timeout: Option<Duration>,
    cancellation: &CancellationToken,
) -> Result<Option<HealthConnection>, LauncherError> {
    let connection = cancel_first(
        cancellation,
        Client::connect(
            product.endpoint_path().as_os_str(),
            None,
            timeout,
            &[],
        ),
    )
    .await
    .ok_or(LauncherError::Cancelled { operation: "connect to daemon health protocol" })?;
    let mut client = match connection {
        Ok(client) => client,
        Err(error) if error.kind() == ClientErrorKind::Connection => return Ok(None),
        Err(error) => {
            return Err(LauncherError::DaemonSpawn(format!(
                "daemon endpoint did not establish authenticated health: {error}",
            )));
        }
    };
    let identity = RequestIdentity::generate()
        .map_err(|error| LauncherError::DaemonSpawn(error.to_string()))?;
    if !client.supports(WellKnownProtocolFeature::DaemonHealth) {
        let response = cancel_first(
            cancellation,
            client.request(identity, AppRequestPayload::DaemonStatus),
        )
        .await
        .ok_or(LauncherError::Cancelled { operation: "request legacy daemon status" })?
        .map_err(|error| LauncherError::DaemonSpawn(format!(
            "legacy daemon status request failed: {error}",
        )))?;
        if !matches!(response.payload(), AppResponsePayload::DaemonStatus(_)) {
            return Err(LauncherError::DaemonSpawn(
                "legacy daemon answered the status request with a different response".to_owned(),
            ));
        }
        return Ok(Some(HealthConnection { client, health: None }));
    }
    let response = cancel_first(
        cancellation,
        client.request(identity, AppRequestPayload::DaemonHealth),
    )
    .await
    .ok_or(LauncherError::Cancelled { operation: "request daemon health" })?
    .map_err(|error| LauncherError::DaemonSpawn(format!(
        "daemon health request failed: {error}",
    )))?;
    let AppResponsePayload::DaemonHealth(health) = response.payload() else {
        return Err(LauncherError::DaemonSpawn(
            "daemon answered the health request with a different response".to_owned(),
        ));
    };
    Ok(Some(HealthConnection { client, health: Some(health.clone()) }))
}

fn classify_health(
    product: &PreparedProduct,
    expected_executable: Sha256Digest,
    expected_implementation: &str,
    connection: &HealthConnection,
    allow_owned_custody: bool,
) -> Result<HealthDisposition, LauncherError> {
    let Some(health) = &connection.health else {
        return Ok(HealthDisposition::Replace {
            active: None,
            stale: held_supervisor_custodies(product)?,
        });
    };
    require_store(product, health)?;
    let instance = health.instance();
    let supervisors = held_supervisor_custodies(product)?;
    let matching = supervisors
        .iter()
        .filter(|custody| custody.configuration_digest == instance.configuration_digest())
        .cloned()
        .collect::<Vec<_>>();
    let receipt = receipt_custody(product, instance, &supervisors)?;
    let custody = receipt.or_else(|| {
        (allow_owned_custody && matching.len() == 1).then(|| matching[0].clone())
    });
    let stale = custody.as_ref().map_or_else(
        || supervisors.clone(),
        |active| {
            supervisors
                .iter()
                .filter(|candidate| *candidate != active)
                .cloned()
                .collect()
        },
    );
    let expected_configuration = product.daemon_config().configuration_digest();
    if connection.client.server_implementation() != expected_implementation
        || instance.configuration_digest() != expected_configuration
        || instance.executable_digest() != expected_executable
    {
        return Ok(HealthDisposition::Replace { active: custody, stale });
    }
    match health.status().readiness() {
        DaemonReadiness::ReadyReadWrite => custody.map_or_else(
            || Ok(HealthDisposition::Replace { active: None, stale: supervisors }),
            |custody| Ok(HealthDisposition::Ready { custody, stale }),
        ),
        DaemonReadiness::Starting | DaemonReadiness::Draining | DaemonReadiness::Unavailable => {
            if custody.is_some() {
                Ok(HealthDisposition::Wait { stale })
            } else {
                Ok(HealthDisposition::Replace { active: None, stale: supervisors })
            }
        }
        DaemonReadiness::ReadyReadOnly => Err(LauncherError::DaemonSpawn(format!(
            "daemon is in read-only recovery: {}",
            health.status().diagnostic().unwrap_or("operator recovery is required"),
        ))),
    }
}

fn require_store(product: &PreparedProduct, health: &DaemonHealth) -> Result<(), LauncherError> {
    let expected = product.daemon_config().store_identity()?;
    if health.instance().store_id() == *expected.as_bytes() {
        Ok(())
    } else {
        Err(LauncherError::DaemonSpawn(
            "authenticated endpoint belongs to a different daemon store".to_owned(),
        ))
    }
}

fn held_supervisor_custodies(
    product: &PreparedProduct,
) -> Result<Vec<SupervisorCustody>, LauncherError> {
    let expected_store = product.daemon_config().store_identity()?;
    let entries = fs::read_dir(product.layout().config_root()).map_err(|error| {
        LauncherError::filesystem(
            "list daemon configuration generations",
            product.layout().config_root(),
            error,
        )
    })?;
    let mut custodians = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            LauncherError::filesystem(
                "read daemon configuration directory entry",
                product.layout().config_root(),
                error,
            )
        })?;
        let Some(generation) = configuration_generation(&entry.file_name()) else { continue };
        let path = entry.path();
        if supervisor_lock_available_at(&path)? {
            continue;
        }
        let text = fs::read_to_string(&path).map_err(|error| {
            LauncherError::filesystem("read supervised daemon configuration", &path, error)
        })?;
        let configuration = peritus_daemon::DaemonConfig::parse(&text)?;
        if configuration.store_identity()? != expected_store {
            continue;
        }
        custodians.push(SupervisorCustody {
            generation,
            configuration: path,
            configuration_digest: configuration.configuration_digest(),
        });
    }
    custodians.sort_unstable_by_key(|custody| custody.generation);
    Ok(custodians)
}

fn configuration_generation(name: &std::ffi::OsStr) -> Option<u64> {
    let name = name.to_str()?;
    let value = name.strip_prefix("peritus-")?.strip_suffix(".toml")?;
    if value.len() != 20 || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let generation = value.parse::<u64>().ok()?;
    (generation > 0 && name == format!("peritus-{generation:020}.toml")).then_some(generation)
}

fn receipt_custody(
    product: &PreparedProduct,
    instance: DaemonInstance,
    supervisors: &[SupervisorCustody],
) -> Result<Option<SupervisorCustody>, LauncherError> {
    let directory = product
        .layout()
        .daemon_reconciliation_lock()
        .parent()
        .ok_or_else(|| {
            LauncherError::PlatformPaths(
                "daemon reconciliation lock has no parent directory".to_owned(),
            )
        })?
        .to_path_buf();
    let suffix = format!(
        "-{:010}-{:020}.receipt",
        instance.process_id(),
        instance.start_token(),
    );
    let entries = fs::read_dir(&directory).map_err(|error| {
        LauncherError::filesystem("list daemon reconciliation receipts", &directory, error)
    })?;
    let mut receipts = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            LauncherError::filesystem("read daemon reconciliation receipt entry", &directory, error)
        })?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else { continue };
        let Some(generation) = name
            .strip_prefix("configuration-reconciliation-")
            .and_then(|value| value.strip_suffix(&suffix))
            .filter(|value| value.len() == 20 && value.bytes().all(|byte| byte.is_ascii_digit()))
            .and_then(|value| value.parse::<u64>().ok())
        else {
            continue;
        };
        receipts.push((generation, entry.path()));
    }
    receipts.sort_unstable_by_key(|(generation, _)| std::cmp::Reverse(*generation));
    let Some((generation, path)) = receipts.first() else { return Ok(None) };
    let text = fs::read_to_string(path).map_err(|error| {
        LauncherError::filesystem("read daemon reconciliation receipt", path, error)
    })?;
    let expected_configuration = hex_digest(instance.configuration_digest().into_bytes());
    let expected_executable = hex_digest(instance.executable_digest().into_bytes());
    let expected_store = hex_bytes(&instance.store_id());
    let mut lines = text.lines();
    let valid = lines.next() == Some("peritus-daemon-reconciliation-v1")
        && receipt_u64(lines.next(), "generation=") == Some(*generation)
        && receipt_field(lines.next(), "configuration-sha256=")
            == Some(expected_configuration.as_str())
        && receipt_field(lines.next(), "executable-sha256=")
            == Some(expected_executable.as_str())
        && receipt_field(lines.next(), "store=") == Some(expected_store.as_str())
        && receipt_u64(lines.next(), "pid=") == Some(u64::from(instance.process_id()))
        && receipt_u64(lines.next(), "start-token=") == Some(instance.start_token());
    let supervisor_generation = receipt_u64(lines.next(), "supervisor-generation=");
    let routes = receipt_field(lines.next(), "routes=");
    let default_route = receipt_field(lines.next(), "default-route=");
    let failover = receipt_field(lines.next(), "automatic-failover=");
    let workspace = receipt_field(lines.next(), "active-workspace=");
    let scope_valid = match (routes, default_route, failover, workspace) {
        (Some(routes), Some(default_route), Some(failover), Some(workspace)) => {
            receipt_scope_matches(
                product,
                *generation,
                routes,
                default_route,
                failover,
                workspace,
            )?
        }
        _ => false,
    };
    if !valid
        || supervisor_generation.is_none()
        || supervisor_generation == Some(0)
        || routes.is_none()
        || default_route.is_none()
        || !matches!(failover, Some("true" | "false"))
        || workspace.is_none()
        || !scope_valid
        || lines.next().is_some()
    {
        return Err(LauncherError::DaemonSpawn(format!(
            "retained daemon reconciliation receipt is malformed: {}",
            path.display(),
        )));
    }
    Ok(supervisors
        .iter()
        .find(|custody| {
            custody.generation == supervisor_generation.unwrap_or_default()
                && custody.configuration_digest == instance.configuration_digest()
        })
        .cloned())
}

fn receipt_scope_matches(
    product: &PreparedProduct,
    generation: u64,
    routes: &str,
    default_route: &str,
    failover: &str,
    active_workspace: &str,
) -> Result<bool, LauncherError> {
    let path = product
        .layout()
        .product_state_root()
        .join(format!("state-{generation:020}.json"));
    let bytes = fs::read(&path).map_err(|error| {
        LauncherError::filesystem("read reconciliation product generation", &path, error)
    })?;
    let state = peritus_product_state::ProductState::parse_json(&bytes)?;
    let expected_routes = state
        .providers()
        .routes()
        .into_iter()
        .map(|route| route.identity().to_string())
        .collect::<Vec<_>>()
        .join(",");
    let expected_default = state
        .providers()
        .default_route()
        .map_or_else(|| "none".to_owned(), |identity| identity.to_string());
    let expected_workspace =
        state.workspaces().active().map_or("none", peritus_product_state::WorkspaceProfile::workspace_id);
    let expected_failover = state.providers().automatic_failover().to_string();
    Ok(state.generation() == generation
        && routes == expected_routes.as_str()
        && default_route == expected_default.as_str()
        && failover == expected_failover.as_str()
        && active_workspace == expected_workspace)
}

fn receipt_field<'a>(line: Option<&'a str>, prefix: &str) -> Option<&'a str> {
    line?.strip_prefix(prefix)
}

fn receipt_u64(line: Option<&str>, prefix: &str) -> Option<u64> {
    receipt_field(line, prefix)?.parse().ok()
}

fn supervisors_released(supervisors: &[SupervisorCustody]) -> Result<bool, LauncherError> {
    for custody in supervisors {
        if !supervisor_lock_available_at(&custody.configuration)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn compatible_supervisor_held(product: &PreparedProduct) -> Result<bool, LauncherError> {
    let expected = product.daemon_config().configuration_digest();
    Ok(held_supervisor_custodies(product)?
        .iter()
        .any(|custody| custody.configuration_digest == expected))
}

async fn settle_unready_supervisors(
    product: &PreparedProduct,
    binaries: &SiblingBinaries,
    cancellation: &CancellationToken,
) -> Result<bool, LauncherError> {
    let supervisors = held_supervisor_custodies(product)?;
    let expected = product.daemon_config().configuration_digest();
    let matching = supervisors
        .iter()
        .filter(|custody| custody.configuration_digest == expected)
        .cloned()
        .collect::<Vec<_>>();
    if matching.len() == 1 {
        let stale = supervisors
            .iter()
            .filter(|custody| *custody != &matching[0])
            .cloned()
            .collect::<Vec<_>>();
        stop_supervisors(binaries, &stale, cancellation).await?;
        return Ok(true);
    }
    stop_supervisors(binaries, &supervisors, cancellation).await?;
    Ok(false)
}

async fn stop_supervisors(
    binaries: &SiblingBinaries,
    supervisors: &[SupervisorCustody],
    cancellation: &CancellationToken,
) -> Result<(), LauncherError> {
    for custody in supervisors {
        if supervisor_lock_available_at(&custody.configuration)? {
            continue;
        }
        let mut command = Command::new(binaries.daemon());
        command.arg("supervise-stop").arg("--config").arg(&custody.configuration);
        let output = bounded_output(command, "stop stale daemon supervisor", cancellation).await?;
        if !output.status.success() {
            return Err(LauncherError::DaemonSpawn(format!(
                "daemon supervisor for generation {} did not stop (status {}, stdout bytes {}, stderr bytes {})",
                custody.generation,
                output.status,
                output.stdout.total_bytes,
                output.stderr.total_bytes,
            )));
        }
    }
    Ok(())
}

async fn request_replacement(
    client: &mut Client,
    launch: Option<&ProductLaunchContext>,
    binaries: &SiblingBinaries,
    active: Option<SupervisorCustody>,
    stale: Vec<SupervisorCustody>,
    cancellation: &CancellationToken,
) -> Result<Vec<SupervisorCustody>, LauncherError> {
    if active.is_some() {
        stop_supervisors(binaries, &stale, cancellation).await?;
    }
    coordinate_retained_scope(client, launch, cancellation).await?;
    request_shutdown(client, cancellation).await?;
    if active.is_none() {
        stop_supervisors(binaries, &stale, cancellation).await?;
    }
    let mut supervisors = stale;
    if let Some(active) = active {
        supervisors.push(active);
    }
    Ok(supervisors)
}

async fn coordinate_retained_scope(
    client: &mut Client,
    launch: Option<&ProductLaunchContext>,
    cancellation: &CancellationToken,
) -> Result<(), LauncherError> {
    let Some(launch) = launch else { return Ok(()) };
    if let Some(run) = launch.run_id() {
        let identity = RequestIdentity::generate()
            .map_err(|error| LauncherError::DaemonSpawn(error.to_string()))?;
        let response = cancel_first(
            cancellation,
            client.request(
                identity,
                AppRequestPayload::QueryProductRunObservations(ProductRunQuery::exact(run)),
            ),
        )
        .await
        .ok_or(LauncherError::Cancelled { operation: "coordinate retained product run" })?
        .map_err(|error| {
            LauncherError::DaemonSpawn(format!(
                "cannot coordinate the retained product run before daemon replacement: {error}",
            ))
        })?;
        let AppResponsePayload::ProductRunObservations(observations) = response.payload() else {
            return Err(LauncherError::DaemonSpawn(
                "daemon did not return the retained product-run ownership observation".to_owned(),
            ));
        };
        if observations.len() > 1
            || observations.first().is_some_and(|observation| {
                observation.snapshot().run_id() != run
                    || observation.snapshot().workspace_id() != launch.workspace_id()
                    || !run_routes_belong_to_launch(observation.snapshot().providers(), launch)
            })
        {
            return Err(LauncherError::DaemonSpawn(
                "daemon returned a different retained product-run scope".to_owned(),
            ));
        }
    }
    if let Some(query) = launch.conversation() {
        if !client.supports(WellKnownProtocolFeature::WorkbenchControl) {
            return Err(LauncherError::DaemonSpawn(
                "daemon cannot verify the retained conversation before replacement".to_owned(),
            ));
        }
        let identity = RequestIdentity::generate()
            .map_err(|error| LauncherError::DaemonSpawn(error.to_string()))?;
        let response = cancel_first(
            cancellation,
            client.request(identity, AppRequestPayload::QueryWorkbench(query)),
        )
        .await
        .ok_or(LauncherError::Cancelled { operation: "coordinate retained conversation" })?
        .map_err(|error| {
            LauncherError::DaemonSpawn(format!(
                "cannot coordinate the retained conversation before daemon replacement: {error}",
            ))
        })?;
        if !matches!(response.payload(), AppResponsePayload::Workbench(snapshot) if snapshot.query() == query)
        {
            return Err(LauncherError::DaemonSpawn(
                "daemon did not return the exact retained conversation scope".to_owned(),
            ));
        }
    }
    Ok(())
}

fn run_routes_belong_to_launch(
    selection: peritus_app_protocol::ProductProviderSelection,
    launch: &ProductLaunchContext,
) -> bool {
    [selection.writer(), selection.reviewer(), selection.fixer()]
        .into_iter()
        .all(|selected| launch.providers().iter().any(|route| route.profile_id() == selected))
}

async fn request_shutdown(
    client: &mut Client,
    cancellation: &CancellationToken,
) -> Result<(), LauncherError> {
    if !client.supports(WellKnownProtocolFeature::GracefulShutdown) {
        return Err(LauncherError::DaemonSpawn(
            "authenticated daemon does not support graceful shutdown".to_owned(),
        ));
    }
    let identity = RequestIdentity::generate()
        .map_err(|error| LauncherError::DaemonSpawn(error.to_string()))?;
    let request = ShutdownRequest::new(identity.request_id, identity.correlation_id);
    let response = cancel_first(
        cancellation,
        client.request(identity, AppRequestPayload::Shutdown(request)),
    )
    .await
    .ok_or(LauncherError::Cancelled { operation: "request daemon shutdown" })?
    .map_err(|error| LauncherError::DaemonSpawn(format!(
        "authenticated daemon shutdown failed: {error}",
    )))?;
    match response.payload() {
        AppResponsePayload::ShutdownAccepted(accepted) if accepted.request() == request => Ok(()),
        _ => Err(LauncherError::DaemonSpawn(
            "daemon did not accept the exact shutdown request".to_owned(),
        )),
    }
}

async fn spawn_daemon(
    binaries: &SiblingBinaries,
    product: &PreparedProduct,
    log_path: &Path,
) -> Result<OwnedSupervisor, LauncherError> {
    let stdout = OpenOptions::new().create(true).append(true).open(log_path).map_err(|error| {
        LauncherError::filesystem("open daemon diagnostic log", log_path, error)
    })?;
    let stderr = stdout.try_clone().map_err(|error| {
        LauncherError::filesystem("clone daemon diagnostic log", log_path, error)
    })?;
    let mut command = Command::new(binaries.daemon());
    command
        .arg("supervise")
        .arg("--config")
        .arg(product.daemon_config_path())
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    detach_from_terminal(&mut command);
    let mut child = command
        .spawn()
        .map_err(|error| LauncherError::DaemonSpawn(error.to_string()))?;
    let process_id = child.id().ok_or_else(|| {
        LauncherError::DaemonSpawn("daemon supervisor did not expose a process id".to_owned())
    })?;
    let tree = match NativeProcessProbe::new().capture_isolated_child(process_id) {
        Ok(tree) => tree,
        Err(error) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(LauncherError::DaemonSpawn(format!(
                "cannot capture daemon supervisor birth identity: {error}",
            )));
        }
    };
    Ok(OwnedSupervisor { child, tree })
}

async fn stop_supervisor(mut owner: OwnedSupervisor) {
    if owner.tree.complete_containment()
        && NativeProcessProbe::new().terminate(owner.tree).is_ok()
    {
        let _ = owner.child.wait().await;
        return;
    }
    if owner.child.try_wait().ok().flatten().is_some() {
        let _ = owner.child.wait().await;
    }
}

fn record_reconciliation_receipt(
    product: &PreparedProduct,
    health: Option<&DaemonHealth>,
    custody: &SupervisorCustody,
) -> Result<(), LauncherError> {
    let health = health.ok_or_else(|| {
        LauncherError::DaemonSpawn(
            "legacy reachability cannot publish an authenticated reconciliation receipt"
                .to_owned(),
        )
    })?;
    let instance = health.instance();
    require_store(product, health)?;
    if instance.configuration_digest() != product.daemon_config().configuration_digest()
        || custody.configuration_digest != instance.configuration_digest()
        || supervisor_lock_available_at(&custody.configuration)?
    {
        return Err(LauncherError::DaemonSpawn(
            "daemon reconciliation lost its exact configuration or supervisor custody"
                .to_owned(),
        ));
    }
    let routes = product
        .state()
        .providers()
        .routes()
        .into_iter()
        .map(|route| route.identity().to_string())
        .collect::<Vec<_>>()
        .join(",");
    let default_route = product
        .state()
        .providers()
        .default_route()
        .map_or_else(|| "none".to_owned(), |identity| identity.to_string());
    let active_workspace = product
        .state()
        .workspaces()
        .active()
        .map_or("none", peritus_product_state::WorkspaceProfile::workspace_id);
    let bytes = format!(
        "peritus-daemon-reconciliation-v1\ngeneration={}\nconfiguration-sha256={}\nexecutable-sha256={}\nstore={}\npid={}\nstart-token={}\nsupervisor-generation={}\nroutes={}\ndefault-route={}\nautomatic-failover={}\nactive-workspace={}\n",
        product.state().generation(),
        hex_digest(instance.configuration_digest().into_bytes()),
        hex_digest(instance.executable_digest().into_bytes()),
        hex_bytes(&instance.store_id()),
        instance.process_id(),
        instance.start_token(),
        custody.generation,
        routes,
        default_route,
        product.state().providers().automatic_failover(),
        active_workspace,
    );
    let path = product.layout().daemon_reconciliation_receipt(
        product.state().generation(),
        instance.process_id(),
        instance.start_token(),
    );
    match fs::read(&path) {
        Ok(existing) if existing == bytes.as_bytes() => return Ok(()),
        Ok(_) => {
            return Err(LauncherError::DaemonSpawn(format!(
                "daemon reconciliation receipt conflicts with retained evidence: {}",
                path.display(),
            )));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(LauncherError::filesystem(
                "read daemon reconciliation receipt",
                path,
                error,
            ));
        }
    }
    publish_new(
        &product.layout().daemon_reconciliation_pending(),
        &path,
        bytes.as_bytes(),
    )
}

fn applied_configuration_matches(
    product: &PreparedProduct,
    binaries: &SiblingBinaries,
) -> Result<bool, LauncherError> {
    let marker = product.layout().daemon_applied_configuration();
    let expected = applied_identity(&product.daemon_config_path(), binaries.daemon())?;
    match fs::read_to_string(&marker) {
        Ok(actual) => Ok(actual == expected),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(LauncherError::filesystem(
            "read applied daemon configuration marker",
            marker,
            error,
        )),
    }
}

fn record_applied_configuration(
    product: &PreparedProduct,
    binaries: &SiblingBinaries,
) -> Result<(), LauncherError> {
    record_applied_configuration_digest(
        product,
        binaries,
        Sha256Digest::new(file_digest(binaries.daemon())?),
    )
}

fn record_applied_configuration_digest(
    product: &PreparedProduct,
    binaries: &SiblingBinaries,
    daemon_digest: Sha256Digest,
) -> Result<(), LauncherError> {
    let marker = product.layout().daemon_applied_configuration();
    let identity = applied_identity_digest(
        &product.daemon_config_path(),
        binaries.daemon(),
        daemon_digest.into_bytes(),
    );
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&marker)
        .map_err(|error| {
            LauncherError::filesystem("open applied daemon configuration marker", &marker, error)
        })?;
    crate::persistence::protect_file(&file, &marker)?;
    file.write_all(identity.as_bytes()).and_then(|()| file.sync_all()).map_err(|error| {
        LauncherError::filesystem("write applied daemon configuration marker", marker, error)
    })
}

fn applied_identity(configuration: &Path, daemon: &Path) -> Result<String, LauncherError> {
    let digest = file_digest(daemon)?;
    Ok(applied_identity_digest(configuration, daemon, digest))
}

fn applied_identity_digest(configuration: &Path, _daemon: &Path, digest: [u8; 32]) -> String {
    format!(
        "peritus-applied-daemon-v2\nconfiguration={}\ndaemon-sha256={}\n",
        configuration.display(),
        hex_digest(digest)
    )
}

fn file_digest(path: &Path) -> Result<[u8; 32], LauncherError> {
    let mut file = File::open(path)
        .map_err(|error| LauncherError::filesystem("open packaged daemon", path, error))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| LauncherError::filesystem("hash packaged daemon", path, error))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().into())
}

async fn file_digest_cancellable(
    path: &Path,
    cancellation: &CancellationToken,
) -> Result<[u8; 32], LauncherError> {
    let mut file = cancel_first(cancellation, tokio::fs::File::open(path))
        .await
        .ok_or(LauncherError::Cancelled { operation: "hash packaged daemon" })?
        .map_err(|error| LauncherError::filesystem("open packaged daemon", path, error))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1_024];
    loop {
        let read = cancel_first(cancellation, file.read(&mut buffer))
            .await
            .ok_or(LauncherError::Cancelled { operation: "hash packaged daemon" })?
            .map_err(|error| LauncherError::filesystem("hash packaged daemon", path, error))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().into())
}

struct BoundedCommandOutput {
    status: ExitStatus,
    stdout: BoundedCapture,
    stderr: BoundedCapture,
}

struct BoundedCapture {
    bytes: Vec<u8>,
    total_bytes: u64,
    truncated: bool,
}

async fn bounded_output(
    mut command: Command,
    operation: &'static str,
    cancellation: &CancellationToken,
) -> Result<BoundedCommandOutput, LauncherError> {
    if cancellation.is_cancelled() {
        return Err(LauncherError::Cancelled { operation });
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    detach_from_terminal(&mut command);
    let mut child = command
        .spawn()
        .map_err(|error| LauncherError::DaemonSpawn(format!("{operation}: {error}")))?;
    let stdout = child.stdout.take().ok_or_else(|| {
        LauncherError::DaemonSpawn(format!("{operation}: stdout pipe is unavailable"))
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        LauncherError::DaemonSpawn(format!("{operation}: stderr pipe is unavailable"))
    })?;
    let stdout_reader = tokio::spawn(drain_bounded(stdout));
    let stderr_reader = tokio::spawn(drain_bounded(stderr));
    let status = match cancel_first(cancellation, child.wait()).await {
        Some(Ok(status)) => status,
        Some(Err(error)) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            let _ = stdout_reader.await;
            let _ = stderr_reader.await;
            return Err(LauncherError::DaemonSpawn(format!(
                "{operation}: wait failed: {error}",
            )));
        }
        None => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            let _ = stdout_reader.await;
            let _ = stderr_reader.await;
            return Err(LauncherError::Cancelled { operation });
        }
    };
    let stdout = join_capture(stdout_reader, operation, "stdout").await?;
    let stderr = join_capture(stderr_reader, operation, "stderr").await?;
    Ok(BoundedCommandOutput { status, stdout, stderr })
}

async fn join_capture(
    reader: tokio::task::JoinHandle<std::io::Result<BoundedCapture>>,
    operation: &'static str,
    stream: &'static str,
) -> Result<BoundedCapture, LauncherError> {
    reader
        .await
        .map_err(|error| {
            LauncherError::DaemonSpawn(format!("{operation}: {stream} reader failed: {error}"))
        })?
        .map_err(|error| {
            LauncherError::DaemonSpawn(format!("{operation}: cannot read {stream}: {error}"))
        })
}

async fn drain_bounded(mut input: impl AsyncRead + Unpin) -> std::io::Result<BoundedCapture> {
    let mut bytes = Vec::new();
    let mut total_bytes = 0_u64;
    let mut buffer = [0_u8; 8 * 1_024];
    loop {
        let read = input.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        total_bytes = total_bytes.saturating_add(u64::try_from(read).unwrap_or(u64::MAX));
        let retained = MAX_RETAINED_PROCESS_OUTPUT_BYTES.saturating_sub(bytes.len()).min(read);
        bytes.extend_from_slice(&buffer[..retained]);
    }
    Ok(BoundedCapture {
        truncated: total_bytes > u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        bytes,
        total_bytes,
    })
}

fn hex_digest(digest: [u8; 32]) -> String {
    hex_bytes(&digest)
}

fn hex_bytes(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

#[cfg(unix)]
fn detach_from_terminal(command: &mut Command) {
    use std::os::unix::process::CommandExt as _;
    command.as_std_mut().process_group(0);
}

#[cfg(windows)]
fn detach_from_terminal(command: &mut Command) {
    use std::os::windows::process::CommandExt as _;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    command.as_std_mut().creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
}

#[cfg(test)]
mod tests;
