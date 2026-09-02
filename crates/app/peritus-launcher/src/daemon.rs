//! Packaged sibling-daemon resolution, startup, and bounded readiness.

use std::{
    fs::{self, File, OpenOptions},
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use sha2::{Digest, Sha256};

use crate::{LauncherError, PreparedProduct};

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
    pub fn discover() -> Result<Self, LauncherError> {
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
        Self::from_paths(current, directory.join(name))
    }

    /// Validates one explicit daemon executable path.
    ///
    /// # Errors
    ///
    /// Returns an actionable packaging error for a missing, non-file, or mismatched executable.
    pub fn from_daemon(daemon: PathBuf) -> Result<Self, LauncherError> {
        let application = std::env::current_exe().map_err(|error| {
            LauncherError::DaemonBinary(format!("cannot locate running executable: {error}"))
        })?;
        Self::from_paths(application, daemon)
    }

    fn from_paths(application: PathBuf, daemon: PathBuf) -> Result<Self, LauncherError> {
        let metadata = fs::metadata(&daemon).map_err(|error| {
            LauncherError::DaemonBinary(format!("{}: {error}", daemon.display()))
        })?;
        if !metadata.is_file() {
            return Err(LauncherError::DaemonBinary(format!(
                "{} is not a regular file",
                daemon.display()
            )));
        }
        let output = Command::new(&daemon).arg("--version").output().map_err(|error| {
            LauncherError::DaemonBinary(format!(
                "cannot execute {} for version check: {error}",
                daemon.display()
            ))
        })?;
        let expected = format!("peritusd {}\n", env!("CARGO_PKG_VERSION"));
        if !output.status.success() || output.stdout != expected.as_bytes() {
            return Err(LauncherError::DaemonBinary(format!(
                "{} is not the matching Peritus {} daemon",
                daemon.display(),
                env!("CARGO_PKG_VERSION")
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

/// Bounded singleton daemon startup and readiness supervisor.
#[derive(Clone, Copy, Debug)]
pub struct DaemonSupervisor {
    readiness_timeout: Duration,
}

impl DaemonSupervisor {
    /// Creates a supervisor with one explicit startup bound.
    #[must_use]
    pub const fn new(readiness_timeout: Duration) -> Self {
        Self { readiness_timeout }
    }

    /// Reuses a reachable endpoint or starts the packaged daemon and waits for readiness.
    ///
    /// # Errors
    ///
    /// Returns spawn, early-exit, timeout, or diagnostic-log failures.
    pub async fn ensure_ready(
        self,
        product: &PreparedProduct,
        binaries: &SiblingBinaries,
    ) -> Result<DaemonLaunch, LauncherError> {
        let endpoint = product.endpoint_path();
        if endpoint_ready(&endpoint).await {
            if applied_configuration_matches(product, binaries)? {
                return Ok(DaemonLaunch::Reused);
            }
            let _stopped = self.shutdown(product, binaries).await?;
        }
        let log_path = product.layout().daemon_log();
        let mut child = spawn_daemon(binaries, product, &log_path)?;
        let process_id = child.id();
        let started = Instant::now();
        loop {
            if endpoint_ready(&endpoint).await {
                record_applied_configuration(product, binaries)?;
                return Ok(DaemonLaunch::Started { process_id });
            }
            if let Some(status) =
                child.try_wait().map_err(|error| LauncherError::DaemonSpawn(error.to_string()))?
            {
                if endpoint_ready(&endpoint).await {
                    if !applied_configuration_matches(product, binaries)? {
                        return Err(LauncherError::DaemonSpawn(
                            "another daemon started with a different product configuration"
                                .to_owned(),
                        ));
                    }
                    return Ok(DaemonLaunch::Reused);
                }
                return Err(LauncherError::DaemonExited { status, log: log_path });
            }
            if started.elapsed() >= self.readiness_timeout {
                stop_child(&mut child);
                return Err(LauncherError::DaemonTimeout {
                    seconds: self.readiness_timeout.as_secs(),
                    log: log_path,
                });
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Requests orderly shutdown through the stable automation surface and waits for withdrawal.
    ///
    /// # Errors
    ///
    /// Returns a bounded process or timeout failure when the reachable daemon does not stop.
    pub async fn shutdown(
        self,
        product: &PreparedProduct,
        binaries: &SiblingBinaries,
    ) -> Result<DaemonShutdown, LauncherError> {
        let endpoint = product.endpoint_path();
        if !endpoint_ready(&endpoint).await {
            return Ok(DaemonShutdown::AlreadyStopped);
        }
        let output = Command::new(binaries.application())
            .arg("--endpoint")
            .arg(&endpoint)
            .arg("shutdown")
            .output()
            .map_err(|error| LauncherError::DaemonSpawn(error.to_string()))?;
        if !output.status.success() {
            return Err(LauncherError::DaemonSpawn(format!(
                "shutdown command failed with status {}",
                output.status
            )));
        }
        let started = Instant::now();
        loop {
            if !endpoint_ready(&endpoint).await && instance_lock_available(product)? {
                return Ok(DaemonShutdown::Stopped);
            }
            if started.elapsed() >= self.readiness_timeout {
                return Err(LauncherError::DaemonTimeout {
                    seconds: self.readiness_timeout.as_secs(),
                    log: product.layout().daemon_log(),
                });
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
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

fn spawn_daemon(
    binaries: &SiblingBinaries,
    product: &PreparedProduct,
    log_path: &Path,
) -> Result<Child, LauncherError> {
    let stdout = OpenOptions::new().create(true).append(true).open(log_path).map_err(|error| {
        LauncherError::filesystem("open daemon diagnostic log", log_path, error)
    })?;
    let stderr = stdout.try_clone().map_err(|error| {
        LauncherError::filesystem("clone daemon diagnostic log", log_path, error)
    })?;
    let mut command = Command::new(binaries.daemon());
    command
        .arg("serve")
        .arg("--config")
        .arg(product.daemon_config_path())
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    detach_from_terminal(&mut command);
    command.spawn().map_err(|error| LauncherError::DaemonSpawn(error.to_string()))
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
    let marker = product.layout().daemon_applied_configuration();
    let identity = applied_identity(&product.daemon_config_path(), binaries.daemon())?;
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
    Ok(format!(
        "peritus-applied-daemon-v2\nconfiguration={}\ndaemon-sha256={}\n",
        configuration.display(),
        hex_digest(digest)
    ))
}

fn file_digest(path: &Path) -> Result<[u8; 32], LauncherError> {
    let mut file = File::open(path)
        .map_err(|error| LauncherError::filesystem("open packaged daemon", path, error))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
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
    command.process_group(0);
}

#[cfg(windows)]
fn detach_from_terminal(command: &mut Command) {
    use std::os::windows::process::CommandExt as _;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
}

fn stop_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(unix)]
async fn endpoint_ready(endpoint: &Path) -> bool {
    tokio::net::UnixStream::connect(endpoint).await.is_ok()
}

#[cfg(windows)]
#[allow(
    clippy::unused_async,
    reason = "keeps Unix sockets and Windows named pipes behind one awaited readiness contract"
)]
async fn endpoint_ready(endpoint: &Path) -> bool {
    let Some(pipe) = endpoint.to_str() else {
        return false;
    };
    tokio::net::windows::named_pipe::ClientOptions::new().open(pipe).is_ok()
}

#[cfg(test)]
mod tests;
