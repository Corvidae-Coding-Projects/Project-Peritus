//! Disposable `peritusd` lifecycle used by integrated qualification campaigns.

use std::fmt::Write as _;
use std::fs::{self, OpenOptions};
use std::io::Read;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use peritus_approval::CredentialRegistrySnapshot;
use peritus_benchmarks::Sha256Digest;
use peritus_types::RevisionNumber;
use sha2::{Digest, Sha256};
use tempfile::{Builder, TempDir};

use crate::SubjectError;

const STARTUP_BOUND: Duration = Duration::from_secs(30);
const EXIT_BOUND: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(20);

pub struct DisposableDaemon {
    _temporary: TempDir,
    executable: PathBuf,
    state_root: PathBuf,
    config_path: PathBuf,
    log_path: PathBuf,
    artifact_path: PathBuf,
    child: Option<Child>,
    endpoint: PathBuf,
}

impl DisposableDaemon {
    pub fn launch(executable: &Path) -> Result<(Self, Duration), SubjectError> {
        let temporary = short_tempdir()?;
        let root = fs::canonicalize(temporary.path())?;
        let state_root = root.join("state");
        let config_path = root.join("daemon.toml");
        let log_path = root.join("daemon.log");
        let artifact_path = root.join("qualification-artifacts.bin");
        let registry_path = root.join("approval-registry.bin");
        let snapshot = CredentialRegistrySnapshot::new(RevisionNumber::first(), Vec::new())
            .map_err(|error| {
                SubjectError::Configuration(format!("approval registry: {error:?}"))
            })?;
        fs::write(
            &registry_path,
            snapshot.canonical_bytes().map_err(|error| {
                SubjectError::Configuration(format!("approval registry encoding: {error:?}"))
            })?,
        )?;
        fs::write(&config_path, configuration_text(&state_root, &registry_path))?;
        let executable = fs::canonicalize(executable)?;
        let started = Instant::now();
        let (child, endpoint) =
            spawn_and_wait(&executable, &config_path, &state_root, &log_path, None)?;
        Ok((
            Self {
                _temporary: temporary,
                executable,
                state_root,
                config_path,
                log_path,
                artifact_path,
                child: Some(child),
                endpoint,
            },
            started.elapsed(),
        ))
    }

    pub fn executable_digest(&self) -> Result<Sha256Digest, SubjectError> {
        digest_file(&self.executable)
    }

    pub fn endpoint(&self) -> &Path {
        &self.endpoint
    }

    pub fn artifact_path(&self) -> &Path {
        &self.artifact_path
    }

    pub fn pid(&self) -> Option<u32> {
        self.child.as_ref().map(Child::id)
    }

    pub fn crash(&mut self) -> Result<(), SubjectError> {
        let Some(mut child) = self.child.take() else {
            return Ok(());
        };
        child.kill()?;
        child.wait()?;
        Ok(())
    }

    pub fn restart(&mut self) -> Result<Duration, SubjectError> {
        self.crash()?;
        let previous = endpoint_identity(&self.endpoint).ok();
        let started = Instant::now();
        let (child, endpoint) = spawn_and_wait(
            &self.executable,
            &self.config_path,
            &self.state_root,
            &self.log_path,
            previous,
        )?;
        self.child = Some(child);
        self.endpoint = endpoint;
        Ok(started.elapsed())
    }
}

impl Drop for DisposableDaemon {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = wait_for_exit(&mut child);
        }
    }
}

fn spawn_and_wait(
    executable: &Path,
    config: &Path,
    state_root: &Path,
    log_path: &Path,
    previous: Option<EndpointIdentity>,
) -> Result<(Child, PathBuf), SubjectError> {
    let log = OpenOptions::new().create(true).append(true).open(log_path)?;
    let stderr = log.try_clone()?;
    let mut child = Command::new(executable)
        .arg("serve")
        .arg("--config")
        .arg(config)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(stderr))
        .spawn()?;
    let deadline = Instant::now() + STARTUP_BOUND;
    loop {
        if let Some(status) = child.try_wait()? {
            return Err(std::io::Error::other(format!(
                "peritusd exited before readiness with {status}: {}",
                bounded_log(log_path)
            ))
            .into());
        }
        if let Some(endpoint) = find_endpoint(state_root)?
            && endpoint_identity(&endpoint).ok() != previous
        {
            return Ok((child, endpoint));
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("peritusd did not publish A3 readiness: {}", bounded_log(log_path)),
            )
            .into());
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn wait_for_exit(child: &mut Child) -> std::io::Result<()> {
    let deadline = Instant::now() + EXIT_BOUND;
    loop {
        if child.try_wait()?.is_some() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "peritusd exit exceeded qualification cleanup bound",
            ));
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn find_endpoint(state_root: &Path) -> std::io::Result<Option<PathBuf>> {
    let entries = match fs::read_dir(state_root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut endpoints = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            entry.file_type().ok().filter(FileTypeExt::is_socket).map(|_| entry.path())
        })
        .collect::<Vec<_>>();
    endpoints.sort();
    match endpoints.as_slice() {
        [] => Ok(None),
        [endpoint] => Ok(Some(endpoint.clone())),
        _ => Err(std::io::Error::other("qualification state has multiple daemon sockets")),
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct EndpointIdentity {
    device: u64,
    inode: u64,
    modified: SystemTime,
}

fn endpoint_identity(path: &Path) -> std::io::Result<EndpointIdentity> {
    let metadata = fs::symlink_metadata(path)?;
    Ok(EndpointIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
        modified: metadata.modified()?,
    })
}

fn short_tempdir() -> std::io::Result<TempDir> {
    Builder::new().prefix("peritus-h3-").tempdir_in(fs::canonicalize("/tmp")?)
}

fn digest_file(path: &Path) -> Result<Sha256Digest, SubjectError> {
    let mut file = fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    let bytes = hash.finalize();
    let mut hex = String::with_capacity(64);
    for byte in bytes {
        write!(&mut hex, "{byte:02x}").expect("writing to String cannot fail");
    }
    Sha256Digest::parse(hex).map_err(SubjectError::Qualification)
}

fn bounded_log(path: &Path) -> String {
    let Ok(bytes) = fs::read(path) else {
        return "daemon log unavailable".to_owned();
    };
    let start = bytes.len().saturating_sub(4_096);
    String::from_utf8_lossy(&bytes[start..]).into_owned()
}

fn configuration_text(state: &Path, registry: &Path) -> String {
    format!(
        r#"version = 1
store_id = "11111111111111111111111111111111"

[paths]
state_root = {state}
artifact_root = {artifacts}
evidence_root = {evidence}
workspace_root = {workspaces}
process_root = {processes}
transaction_root = {transactions}
backup_root = {backups}

[approval_registry]
payload_file = {registry}
generation = 1

[limits]
authority_queue = 512
connection_queue = 64
maximum_connections = 16
maximum_workers = 16
maximum_artifact_bytes = 16777216
artifact_quota_bytes = 8589934592
shutdown_millis = 5000

[human]
actor_id = "22222222222222222222222222222222"

[telemetry]
mode = "disabled"
"#,
        state = quote(state),
        artifacts = quote(&state.join("artifacts")),
        evidence = quote(&state.join("evidence")),
        workspaces = quote(&state.join("workspaces")),
        processes = quote(&state.join("processes")),
        transactions = quote(&state.join("transactions")),
        backups = quote(&state.join("backups")),
        registry = quote(registry),
    )
}

fn quote(path: &Path) -> String {
    format!("{:?}", path.to_string_lossy())
}
