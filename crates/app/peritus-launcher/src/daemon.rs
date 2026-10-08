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
    AppRequestPayload, AppResponsePayload, DaemonHealth, DaemonReadiness, ShutdownRequest,
    WellKnownProtocolFeature,
};
use peritus_process::{NativeProcessProbe, ProcessProbe, ProcessTreeIdentity};
use peritus_provider_core::{CancellationToken, first as cancel_first};
use peritus_types::Sha256Digest;
use sha2::{Digest, Sha256};
use tokio::{
    io::{AsyncRead, AsyncReadExt as _},
    process::{Child, Command},
};

use crate::{LauncherError, PreparedProduct};

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
            )? {
                HealthDisposition::Ready => {
                    record_applied_configuration_digest(product, binaries, executable_digest)?;
                    return Ok(DaemonLaunch::Reused);
                }
                HealthDisposition::Replace => {
                    request_shutdown(&mut connection.client, cancellation).await?;
                    self.wait_for_withdrawal(product, started, cancellation).await?;
                }
                HealthDisposition::Wait => {
                    if let Some(launch) = self
                        .wait_for_existing(
                            product,
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

        if (!instance_lock_available(product)? || !supervisor_lock_available(product)?)
            && let Some(launch) = self
                .wait_for_existing(
                    product,
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
                )? {
                    HealthDisposition::Ready => {
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
                        record_applied_configuration_digest(
                            product,
                            binaries,
                            executable_digest,
                        )?;
                        return Ok(DaemonLaunch::Started { process_id });
                    }
                    HealthDisposition::Replace => {
                        request_shutdown(&mut connection.client, cancellation).await?;
                        self.wait_for_withdrawal(product, started, cancellation).await?;
                        return Err(LauncherError::DaemonSpawn(
                            "started daemon reported a different store, configuration, or executable identity"
                                .to_owned(),
                        ));
                    }
                    HealthDisposition::Wait => {}
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
        self.wait_for_withdrawal(product, started, cancellation).await?;
        Ok(DaemonShutdown::Stopped)
    }

    async fn wait_for_existing(
        &mut self,
        product: &PreparedProduct,
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
                )? {
                    HealthDisposition::Ready => {
                        record_applied_configuration_digest(
                            product,
                            binaries,
                            executable_digest,
                        )?;
                        return Ok(Some(DaemonLaunch::Reused));
                    }
                    HealthDisposition::Replace => {
                        request_shutdown(&mut connection.client, cancellation).await?;
                        self.wait_for_withdrawal(product, started, cancellation).await?;
                        return Ok(None);
                    }
                    HealthDisposition::Wait => {}
                },
                None
                    if instance_lock_available(product)?
                        && supervisor_lock_available(product)? =>
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
        started: Instant,
        cancellation: &CancellationToken,
    ) -> Result<(), LauncherError> {
        let log_path = product.layout().daemon_log();
        loop {
            if instance_lock_available(product)? && supervisor_lock_available(product)?
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HealthDisposition {
    Ready,
    Wait,
    Replace,
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
    let mut path = product.daemon_config_path().into_os_string();
    path.push(".supervisor.lock");
    let path = PathBuf::from(path);
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(|error| LauncherError::filesystem("open daemon supervisor lock", &path, error))?;
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
) -> Result<HealthDisposition, LauncherError> {
    let Some(health) = &connection.health else {
        return Ok(HealthDisposition::Replace);
    };
    require_store(product, health)?;
    let instance = health.instance();
    let expected_configuration = product.daemon_config().configuration_digest();
    if connection.client.server_implementation() != expected_implementation
        || instance.configuration_digest() != expected_configuration
        || instance.executable_digest() != expected_executable
    {
        return Ok(HealthDisposition::Replace);
    }
    match health.status().readiness() {
        DaemonReadiness::ReadyReadWrite => Ok(HealthDisposition::Ready),
        DaemonReadiness::Starting | DaemonReadiness::Draining | DaemonReadiness::Unavailable => {
            Ok(HealthDisposition::Wait)
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
    let mut output = String::with_capacity(64);
    for byte in digest {
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
