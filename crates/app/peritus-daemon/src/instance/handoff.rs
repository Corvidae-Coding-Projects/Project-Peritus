//! Exact-instance durable handoff for package publication.

mod records;

use records::{completion_bytes, completion_status, handoff_path, hex, latest_path, publish, publish_latest};

use std::{
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::Duration,
};

use peritus_app_protocol::ShutdownCompletionDisposition;

use super::record::InstanceRecord;
use crate::{
    DaemonConfig, DaemonError, DaemonErrorCode, DaemonIdentity, DaemonRecovery, ShutdownOutcome,
};

const POLL_INTERVAL: Duration = Duration::from_millis(100);

pub(crate) struct HandoffOwner {
    state_root: PathBuf,
    record: InstanceRecord,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HandoffCompletion {
    bytes: Vec<u8>,
}

impl HandoffOwner {
    pub(crate) fn current(config: &DaemonConfig) -> Result<Self, DaemonError> {
        let identity = DaemonIdentity::new(config.store_identity()?);
        let record = InstanceRecord::current(&identity)?;
        Ok(Self { state_root: config.paths().state_root().to_path_buf(), record })
    }

    pub(crate) async fn wait(&self) -> Result<(), DaemonError> {
        let request = self.request_path();
        loop {
            let read_path = request.clone();
            let read = tokio::task::spawn_blocking(move || fs::read(read_path))
                .await
                .map_err(|error| {
                    storage("join package handoff read", std::io::Error::other(error))
                })?;
            match read {
                Ok(bytes) if bytes == self.record.bytes() => return Ok(()),
                Ok(_) => return Err(corrupt("package handoff request has conflicting identity")),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(storage("read package handoff request", error)),
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }

    pub(crate) fn complete(
        &self,
        wait_succeeded: bool,
        outcome: &Result<ShutdownOutcome, DaemonError>,
    ) -> Result<(), DaemonError> {
        match fs::read(self.request_path()) {
            Ok(bytes) if bytes == self.record.bytes() => {}
            Ok(_) => return Err(corrupt("package handoff request has conflicting identity")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(storage("read package handoff request", error)),
        }
        let disposition = match outcome {
            Ok(outcome)
                if wait_succeeded
                    && outcome.disposition() == ShutdownCompletionDisposition::Clean =>
            {
                "clean"
            }
            Ok(_) => "unclean",
            Err(_) => "failed",
        };
        let bytes = completion_bytes(&self.record, disposition);
        publish_latest(&self.state_root, &bytes)?;
        publish(&self.state_root, &self.completion_path(), &bytes)
    }

    fn request_path(&self) -> PathBuf {
        handoff_path(&self.state_root, &self.record, "request")
    }

    fn completion_path(&self) -> PathBuf {
        handoff_path(&self.state_root, &self.record, "complete")
    }
}

pub(crate) fn request_handoff(config: &DaemonConfig) -> Result<HandoffCompletion, DaemonError> {
    let identity = DaemonIdentity::new(config.store_identity()?);
    let state_root = config.paths().state_root();
    let record = loop {
        match InstanceRecord::read(state_root, &identity)? {
            Some(record) => break record,
            None if ownership_released(state_root)? => {
                return stopped_completion(config, &identity);
            }
            None => thread::sleep(POLL_INTERVAL),
        }
    };
    if !record.is_live() {
        if ownership_released(state_root)? {
            return stopped_completion(config, &identity);
        }
        return Err(corrupt("daemon lock is owned by a process other than its recorded identity"));
    }
    if ownership_released(state_root)? {
        return Err(corrupt("live daemon instance record has no matching lock owner"));
    }

    let request = handoff_path(state_root, &record, "request");
    publish(state_root, &request, record.bytes())?;
    let completion = handoff_path(state_root, &record, "complete");
    loop {
        match fs::read(&completion) {
            Ok(bytes) => {
                let expected = completion_bytes(&record, "clean");
                if bytes != expected {
                    return Err(corrupt("daemon rejected clean package handoff"));
                }
                while !ownership_released(state_root)? {
                    let observed = InstanceRecord::read(state_root, &identity)?;
                    verify_owner_transition(observed.as_ref(), &record)?;
                    thread::sleep(POLL_INTERVAL);
                }
                return Ok(HandoffCompletion { bytes });
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(storage("read package handoff completion", error)),
        }
        if ownership_released(state_root)? {
            return Err(corrupt(
                "daemon released ownership without a clean package handoff receipt",
            ));
        }
        let observed = InstanceRecord::read(state_root, &identity)?;
        verify_owner_transition(observed.as_ref(), &record)?;
        thread::sleep(POLL_INTERVAL);
    }
}

/// Coordinates a clean handoff from a daemon generation that predates the retained request file.
///
/// The packaged client waits for the daemon's correlated shutdown completion event. The exact
/// PID and operating-system birth token captured before that request must then release the store
/// lock before this command can issue a clean receipt.
pub(crate) fn request_legacy_handoff(
    config: &DaemonConfig,
) -> Result<HandoffCompletion, DaemonError> {
    let identity = DaemonIdentity::new(config.store_identity()?);
    let state_root = config.paths().state_root();
    let Some(record) = InstanceRecord::read(state_root, &identity)? else {
        return if ownership_released(state_root)? {
            stopped_completion(config, &identity)
        } else {
            Err(corrupt("daemon lock has no exact instance record"))
        };
    };
    if !record.is_live() || ownership_released(state_root)? {
        return Err(corrupt("daemon instance record does not identify the lock owner"));
    }

    let daemon = std::env::current_exe()
        .map_err(|error| storage("resolve packaged handoff helper", error))?;
    let application = daemon
        .parent()
        .ok_or_else(|| corrupt("packaged handoff helper has no binary directory"))?
        .join(if cfg!(windows) { "peritus.exe" } else { "peritus" });
    let status = Command::new(application)
        .arg("--endpoint")
        .arg(record.endpoint())
        .arg("shutdown")
        .arg("--wait")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .status()
        .map_err(|error| storage("run packaged graceful-shutdown client", error))?;
    if !status.success() {
        publish_latest(state_root, &completion_bytes(&record, "failed"))?;
        return Err(corrupt(
            "legacy daemon did not acknowledge a clean graceful-shutdown completion",
        ));
    }

    if let Err(error) = wait_for_release(state_root, &identity, &record) {
        publish_latest(state_root, &completion_bytes(&record, "failed"))?;
        return Err(error);
    }
    let bytes = completion_bytes(&record, "clean");
    publish_latest(state_root, &bytes)?;
    Ok(HandoffCompletion { bytes })
}

impl HandoffCompletion {
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

fn stopped_completion(
    config: &DaemonConfig,
    identity: &DaemonIdentity,
) -> Result<HandoffCompletion, DaemonError> {
    match fs::read(latest_path(config.paths().state_root())) {
        Ok(bytes) => {
            let status = completion_status(&bytes, identity)?;
            if status != "clean" {
                return Err(corrupt(
                    "the last requested package handoff did not complete cleanly",
                ));
            }
            return Ok(HandoffCompletion { bytes });
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(storage("read retained package handoff outcome", error)),
    }
    Ok(HandoffCompletion {
        bytes: format!(
            "peritus-package-handoff-v1\nstore={}\nstatus=already-stopped\n",
            hex(config.store_identity()?.as_bytes()),
        )
        .into_bytes(),
    })
}

fn wait_for_release(
    state_root: &Path,
    identity: &DaemonIdentity,
    record: &InstanceRecord,
) -> Result<(), DaemonError> {
    while !ownership_released(state_root)? {
        let observed = InstanceRecord::read(state_root, identity)?;
        verify_owner_transition(observed.as_ref(), record)?;
        thread::sleep(POLL_INTERVAL);
    }
    Ok(())
}

fn verify_owner_transition(
    observed: Option<&InstanceRecord>,
    expected: &InstanceRecord,
) -> Result<(), DaemonError> {
    if !expected.is_live() {
        return Err(corrupt("daemon identity exited before ownership was released"));
    }
    if observed.is_some_and(|observed| observed != expected) {
        return Err(corrupt("daemon identity changed before ownership was released"));
    }
    // InstanceGuard removes the exact record before unlocking daemon.lock. A missing record while
    // the captured PID and birth token remain live is therefore the expected teardown transition.
    Ok(())
}

fn ownership_released(state_root: &Path) -> Result<bool, DaemonError> {
    let path = state_root.join("daemon.lock");
    let file = match OpenOptions::new().read(true).write(true).open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Err(error) => return Err(storage("open daemon instance lock", error)),
    };
    match file.try_lock() {
        Ok(()) => {
            file.unlock().map_err(|error| storage("release daemon instance lock probe", error))?;
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(false),
        Err(error) => Err(storage("probe daemon instance lock", error)),
    }
}

fn corrupt(detail: &'static str) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::RecoveryRequired,
        DaemonRecovery::Operator,
        "coordinate package handoff",
        detail,
    )
}

fn storage(operation: &'static str, error: std::io::Error) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Storage,
        DaemonRecovery::Retry,
        operation,
        "package handoff evidence cannot be durably updated",
        error,
    )
}
