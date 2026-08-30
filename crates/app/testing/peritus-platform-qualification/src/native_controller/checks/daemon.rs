//! Real packaged daemon startup, authenticated CLI readiness, and bounded teardown.

use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use peritus_approval::CredentialRegistrySnapshot;
use peritus_types::RevisionNumber;
use sha2::{Digest as _, Sha256};

use super::host::{HostLayout, command_output, require_success};

const READY_TIMEOUT: Duration = Duration::from_secs(20);
const STORE_ID: [u8; 16] = [0x31; 16];

pub(super) struct DaemonSession<'a> {
    layout: &'a HostLayout,
    child: Option<Child>,
    endpoint: OsString,
    endpoint_path: Option<PathBuf>,
    log: PathBuf,
}

impl<'a> DaemonSession<'a> {
    pub(super) fn start(layout: &'a HostLayout) -> Result<Self, Box<dyn std::error::Error>> {
        let runtime = runtime_root(layout)?;
        fs::create_dir_all(&runtime)?;
        let state = runtime.join("state");
        let registry_path = runtime.join("approval-registry.bin");
        let registry = CredentialRegistrySnapshot::new(RevisionNumber::first(), Vec::new())
            .map_err(|error| approval_error("construct", error))?;
        let registry_bytes =
            registry.canonical_bytes().map_err(|error| approval_error("encode", error))?;
        fs::write(&registry_path, registry_bytes)?;
        let config = runtime.join("peritus.toml");
        fs::write(&config, render_configuration(&state, &registry_path))?;
        let endpoint_name = endpoint_name();
        let (endpoint, endpoint_path) = if cfg!(windows) {
            (OsString::from(format!(r"\\.\pipe\{endpoint_name}")), None)
        } else {
            let path = state.join(format!("{endpoint_name}.sock"));
            (path.as_os_str().to_owned(), Some(path))
        };
        let log = runtime.join("daemon.log");
        let stdout = OpenOptions::new().create(true).append(true).open(&log)?;
        let stderr = stdout.try_clone()?;
        let child = Command::new(&layout.daemon)
            .arg("serve")
            .arg("--config")
            .arg(&config)
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr))
            .spawn()?;
        let mut session = Self { layout, child: Some(child), endpoint, endpoint_path, log };
        let started = Instant::now();
        loop {
            if session.status().is_ok() {
                return Ok(session);
            }
            if session.child.as_mut().is_some_and(|child| child.try_wait().ok().flatten().is_some())
            {
                return Err(format!(
                    "packaged daemon exited before native readiness: {}",
                    session.diagnostics()
                )
                .into());
            }
            if started.elapsed() >= READY_TIMEOUT {
                session.kill()?;
                return Err(format!(
                    "packaged daemon did not become ready within 20 seconds: {}",
                    session.diagnostics()
                )
                .into());
            }
            thread::sleep(Duration::from_millis(100));
        }
    }

    pub(super) fn endpoint_path(&self) -> Option<&Path> {
        self.endpoint_path.as_deref()
    }

    pub(super) fn status(&self) -> Result<(), Box<dyn std::error::Error>> {
        let output = command_output(
            &self.layout.cli,
            [
                OsString::from("--endpoint"),
                self.endpoint.clone(),
                OsString::from("--timeout-seconds"),
                OsString::from("1"),
                OsString::from("status"),
            ],
        )?;
        require_success(&output, "query packaged daemon status")
    }

    pub(super) fn shutdown(mut self) -> Result<(), Box<dyn std::error::Error>> {
        let output = command_output(
            &self.layout.cli,
            [
                OsString::from("--endpoint"),
                self.endpoint.clone(),
                OsString::from("--timeout-seconds"),
                OsString::from("5"),
                OsString::from("shutdown"),
                OsString::from("--wait"),
            ],
        )?;
        require_success(&output, "shut down packaged daemon")?;
        let status = self.wait_for_exit()?;
        if !status.success() {
            return Err(format!(
                "packaged daemon reported an unclean shutdown with status {status}: {}",
                self.diagnostics()
            )
            .into());
        }
        if self.endpoint_path.as_ref().is_some_and(|path| path.exists()) {
            return Err("packaged daemon exited without withdrawing its local endpoint".into());
        }
        Ok(())
    }

    pub(super) fn kill(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(mut child) = self.child.take() {
            if child.try_wait()?.is_none() {
                child.kill()?;
            }
            child.wait()?;
        }
        Ok(())
    }

    fn wait_for_exit(&mut self) -> Result<ExitStatus, Box<dyn std::error::Error>> {
        let Some(mut child) = self.child.take() else {
            return Err("packaged daemon process was already reaped".into());
        };
        let started = Instant::now();
        loop {
            if let Some(status) = child.try_wait()? {
                return Ok(status);
            }
            if started.elapsed() >= READY_TIMEOUT {
                child.kill()?;
                child.wait()?;
                return Err("packaged daemon did not stop within 20 seconds".into());
            }
            thread::sleep(Duration::from_millis(50));
        }
    }

    fn diagnostics(&self) -> String {
        fs::read(&self.log).map_or_else(
            |error| format!("diagnostic log unavailable: {error}"),
            |bytes| {
                let start = bytes.len().saturating_sub(8 * 1024);
                String::from_utf8_lossy(&bytes[start..]).into_owned()
            },
        )
    }
}

pub(super) fn cleanup_runtime(layout: &HostLayout) -> Result<(), Box<dyn std::error::Error>> {
    let root = runtime_root(layout)?;
    match fs::remove_dir_all(root) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

impl Drop for DaemonSession<'_> {
    fn drop(&mut self) {
        let _ = self.kill();
    }
}

fn render_configuration(state: &Path, registry: &Path) -> String {
    format!(
        "version = 1\nstore_id = \"{}\"\n\n[paths]\nstate_root = {}\nartifact_root = {}\nevidence_root = {}\nworkspace_root = {}\nprocess_root = {}\ntransaction_root = {}\nbackup_root = {}\n\n[approval_registry]\npayload_file = {}\ngeneration = 1\n\n[human]\nactor_id = \"{}\"\n\n[product]\nautomatic_provider_failover = false\n\n[telemetry]\nmode = \"disabled\"\n\n[tools]\nallow = []\n",
        hex(&STORE_ID),
        toml_path(state),
        toml_path(&state.join("artifacts")),
        toml_path(&state.join("evidence")),
        toml_path(&state.join("workspaces")),
        toml_path(&state.join("processes")),
        toml_path(&state.join("transactions")),
        toml_path(&state.join("backups")),
        toml_path(registry),
        "32".repeat(16),
    )
}

#[cfg(unix)]
fn runtime_root(_layout: &HostLayout) -> Result<PathBuf, Box<dyn std::error::Error>> {
    // macOS spells its temporary directory through `/tmp`, an operating-system symlink to
    // `/private/tmp`. The daemon deliberately rejects aliased state roots, so bind the native
    // qualification runtime to the canonical spelling without weakening daemon validation.
    Ok(native_temporary_parent()?.join(format!("ph2-{}", std::process::id())))
}

#[cfg(windows)]
fn runtime_root(layout: &HostLayout) -> Result<PathBuf, Box<dyn std::error::Error>> {
    fs::create_dir_all(&layout.state)?;
    Ok(fs::canonicalize(&layout.state)?.join("h2-daemon-runtime"))
}

#[cfg(unix)]
fn native_temporary_parent() -> Result<PathBuf, std::io::Error> {
    fs::canonicalize("/tmp")
}

fn endpoint_name() -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"peritus/daemon-endpoint/v1\0");
    hasher.update(STORE_ID);
    format!("peritus-{}", hex(&hasher.finalize()[..16]))
}

fn toml_path(path: &Path) -> String {
    format!("{:?}", path.to_string_lossy())
}

fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

fn approval_error(action: &str, error: impl std::fmt::Debug) -> std::io::Error {
    std::io::Error::other(format!("{action} H2 approval registry: {error:?}"))
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::path::Path;

    #[cfg(unix)]
    #[test]
    fn native_runtime_parent_uses_its_canonical_spelling() {
        let parent =
            super::native_temporary_parent().expect("native temporary directory must resolve");
        assert_eq!(
            std::fs::canonicalize(&parent).expect("canonical temporary directory must resolve"),
            parent
        );
        assert!(parent.is_absolute());
        assert_ne!(parent, Path::new(""));
    }
}
