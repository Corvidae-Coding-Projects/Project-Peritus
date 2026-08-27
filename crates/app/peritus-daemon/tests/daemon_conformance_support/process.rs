//! Isolated `peritusd` process and filesystem ownership.

use std::fs::{self, OpenOptions};
use std::io;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use peritus_approval::CredentialRegistrySnapshot;
use peritus_types::RevisionNumber;
use tempfile::TempDir;

const PROCESS_BOUND: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(20);

pub(super) struct TestEnvironment {
    _temporary: TempDir,
    state_root: PathBuf,
    config_path: PathBuf,
    log_path: PathBuf,
}

impl TestEnvironment {
    pub(super) fn new() -> io::Result<Self> {
        let temporary = TempDir::new()?;
        let root = temporary.path().to_path_buf();
        let state_root = root.join("state");
        let config_path = root.join("daemon.toml");
        let log_path = root.join("daemon.log");
        let registry_path = root.join("approval-registry.bin");
        let snapshot = CredentialRegistrySnapshot::new(RevisionNumber::first(), Vec::new())
            .map_err(super::debug_error)?;
        fs::write(&registry_path, snapshot.canonical_bytes().map_err(super::debug_error)?)?;
        fs::write(&config_path, configuration_text(&state_root, &registry_path))?;
        Ok(Self { _temporary: temporary, state_root, config_path, log_path })
    }

    pub(super) fn start(&self) -> io::Result<DaemonProcess> {
        let previous_endpoint =
            find_endpoint(&self.state_root)?.as_deref().map(endpoint_identity).transpose()?;
        let child = self.spawn_child()?;
        DaemonProcess::wait_until_ready(
            child,
            &self.state_root,
            self.log_path.clone(),
            previous_endpoint,
        )
    }

    pub(super) fn spawn_competitor(&self) -> io::Result<Child> {
        self.spawn_child()
    }

    pub(super) fn wait_for_exit(child: &mut Child) -> io::Result<ExitStatus> {
        wait_for_exit(child, PROCESS_BOUND)
    }

    pub(super) fn state_root(&self) -> &Path {
        &self.state_root
    }

    pub(super) fn config_path(&self) -> &Path {
        &self.config_path
    }

    pub(super) fn database_path(&self) -> PathBuf {
        self.state_root.join("peritus.sqlite3")
    }

    pub(super) fn prepare_corrupt_process_record(&self) -> io::Result<()> {
        let process_root = self.state_root.join("processes");
        let workspace_root = self.state_root.join("workspaces");
        fs::create_dir_all(&workspace_root)?;
        let store = peritus_process::ProcessStore::open(&process_root, &workspace_root)
            .map_err(super::debug_error)?;
        let manifest_directory = fs::read_dir(store.root())?
            .filter_map(Result::ok)
            .find(|entry| {
                entry.file_type().is_ok_and(|kind| kind.is_dir())
                    && entry.file_name().to_string_lossy().starts_with("manifests-")
            })
            .map(|entry| entry.path())
            .ok_or_else(|| io::Error::other("process store exposed no manifest directory"))?;
        fs::write(
            manifest_directory.join("corrupt-recovery.manifest"),
            b"not a canonical process manifest",
        )
    }

    fn spawn_child(&self) -> io::Result<Child> {
        let log = OpenOptions::new().create(true).append(true).open(&self.log_path)?;
        let stderr = log.try_clone()?;
        Command::new(peritusd_executable()?)
            .arg("serve")
            .arg("--config")
            .arg(&self.config_path)
            .stdin(Stdio::null())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(stderr))
            .spawn()
    }
}

pub(super) fn peritusd_executable() -> io::Result<PathBuf> {
    std::env::current_exe()?
        .parent()
        .and_then(Path::parent)
        .map(|directory| directory.join("peritusd"))
        .ok_or_else(|| io::Error::other("integration-test executable has no Cargo target parent"))
}

pub(super) struct DaemonProcess {
    child: Option<Child>,
    endpoint: PathBuf,
    log_path: PathBuf,
}

impl DaemonProcess {
    fn wait_until_ready(
        mut child: Child,
        state_root: &Path,
        log_path: PathBuf,
        previous_endpoint: Option<(u64, u64)>,
    ) -> io::Result<Self> {
        let deadline = Instant::now() + PROCESS_BOUND;
        loop {
            if let Some(status) = child.try_wait()? {
                return Err(io::Error::other(format!(
                    "peritusd exited before IPC readiness with {status}: {}",
                    bounded_log(&log_path),
                )));
            }
            if let Some(endpoint) = find_endpoint(state_root)?
                && Some(endpoint_identity(&endpoint)?) != previous_endpoint
            {
                return Ok(Self { child: Some(child), endpoint, log_path });
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("peritusd did not publish IPC in time: {}", bounded_log(&log_path)),
                ));
            }
            thread::sleep(POLL_INTERVAL);
        }
    }

    pub(super) fn endpoint(&self) -> &Path {
        &self.endpoint
    }

    pub(super) fn diagnostic(&mut self) -> String {
        let status = self
            .child
            .as_mut()
            .and_then(|child| child.try_wait().ok().flatten())
            .map_or_else(|| "running".to_owned(), |status| status.to_string());
        format!("status={status}; log={}", bounded_log(&self.log_path))
    }

    pub(super) fn kill_for_restart(&mut self) -> io::Result<()> {
        let Some(mut child) = self.child.take() else {
            return Ok(());
        };
        child.kill()?;
        let _ = child.wait()?;
        Ok(())
    }

    pub(super) fn wait_for_clean_exit(&mut self) -> io::Result<ExitStatus> {
        let Some(mut child) = self.child.take() else {
            return Err(io::Error::other("peritusd child is no longer owned"));
        };
        let status = wait_for_exit(&mut child, PROCESS_BOUND)?;
        if status.success() {
            Ok(status)
        } else {
            Err(io::Error::other(format!(
                "peritusd exited uncleanly with {status}: {}",
                bounded_log(&self.log_path),
            )))
        }
    }
}

impl Drop for DaemonProcess {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn wait_for_exit(child: &mut Child, bound: Duration) -> io::Result<ExitStatus> {
    let deadline = Instant::now() + bound;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::new(io::ErrorKind::TimedOut, "peritusd exit exceeded bound"));
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn find_endpoint(state_root: &Path) -> io::Result<Option<PathBuf>> {
    let entries = match fs::read_dir(state_root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
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
        _ => Err(io::Error::other("state root contains multiple daemon sockets")),
    }
}

fn endpoint_identity(path: &Path) -> io::Result<(u64, u64)> {
    let metadata = fs::symlink_metadata(path)?;
    Ok((metadata.dev(), metadata.ino()))
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
authority_queue = 64
connection_queue = 32
maximum_connections = 8
maximum_workers = 4
maximum_artifact_bytes = 65536
artifact_quota_bytes = 1048576
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
