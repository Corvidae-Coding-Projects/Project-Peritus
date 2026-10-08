//! Nonblocking run-level memory and resumable workspace observations.

use std::fs::{self, ReadDir};
use std::path::{Path, PathBuf};
#[cfg(all(unix, not(target_os = "linux")))]
use std::process::{Command, Stdio};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};

use crate::{
    ProductRunnerError,
    accounting::{
        ResourceIoErrorKind, ResourceMeasurement, ResourceMeasurementStatus,
        ResourceObservationCause, ResourceObservationCoverage, ResourceObservationOperation,
    },
};

/// This limits one worker publication, not the total traversal. The retained directory cursor
/// continues until every reachable entry has been visited or cancellation is observed.
const WORKSPACE_TRANSFER_PAGE: usize = 4_096;
#[cfg(target_os = "macos")]
const PROCESS_STATUS_COMMAND: &str = "/bin/ps";
#[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
const PROCESS_STATUS_COMMAND: &str = "ps";

pub(super) struct RunResourceProbe {
    observation: Arc<Mutex<RunResourceObservation>>,
    refresh_requested: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

#[derive(Clone, Copy)]
pub(super) struct RunResourceObservation {
    /// Compatibility projection. Consult `workspace_measurement` before treating this as exact.
    pub(super) workspace: u64,
    /// Compatibility projection. Consult `growth_measurement` before treating this as exact.
    pub(super) growth: u64,
    /// Last defensible observed high-water value; availability is carried separately.
    pub(super) peak_rss: u64,
    pub(super) workspace_measurement: ResourceMeasurement,
    pub(super) growth_measurement: ResourceMeasurement,
    pub(super) peak_rss_measurement: ResourceMeasurement,
}

impl RunResourceProbe {
    #[cfg(test)]
    pub(super) fn new(workspace_root: &Path) -> Result<Self, ProductRunnerError> {
        Ok(Self::with_cancellation(
            Some(workspace_root.to_owned()),
            Arc::new(AtomicBool::new(false)),
        ))
    }

    pub(super) fn new_with_cancellation(
        workspace_root: &Path,
        cancelled: Arc<AtomicBool>,
    ) -> Self {
        Self::with_cancellation(Some(workspace_root.to_owned()), cancelled)
    }

    /// Direct folders do not authorize a recursive home-directory inventory for accounting.
    pub(super) fn process_only(cancelled: Arc<AtomicBool>) -> Self {
        Self::with_cancellation(None, cancelled)
    }

    fn with_cancellation(workspace_root: Option<PathBuf>, cancelled: Arc<AtomicBool>) -> Self {
        let workspace_cause = if workspace_root.is_some() {
            ResourceObservationCause::NotObserved
        } else {
            ResourceObservationCause::NotAuthorized
        };
        let initial = RunResourceObservation {
            workspace: 0,
            growth: 0,
            peak_rss: 0,
            workspace_measurement: ResourceMeasurement::unavailable(workspace_cause),
            growth_measurement: ResourceMeasurement::unavailable(workspace_cause),
            peak_rss_measurement: ResourceMeasurement::unavailable(
                ResourceObservationCause::NotObserved,
            ),
        };
        let observation = Arc::new(Mutex::new(initial));
        let refresh_requested = Arc::new(AtomicBool::new(true));
        let stopped = Arc::new(AtomicBool::new(false));
        let worker_observation = Arc::clone(&observation);
        let worker_refresh = Arc::clone(&refresh_requested);
        let worker_stopped = Arc::clone(&stopped);
        let worker = thread::Builder::new()
            .name("peritus-resource-observer".to_owned())
            .spawn(move || {
                observe_resources(
                    workspace_root,
                    worker_observation,
                    worker_refresh,
                    worker_stopped,
                    cancelled,
                );
            });
        match worker {
            Ok(worker) => Self {
                observation,
                refresh_requested,
                stopped,
                worker: Some(worker),
            },
            Err(error) => {
                let cause =
                    io_cause(ResourceObservationOperation::StartResourceObserver, &error);
                if let Ok(mut value) = observation.lock() {
                    value.workspace_measurement = ResourceMeasurement::unavailable(cause);
                    value.growth_measurement = ResourceMeasurement::unavailable(cause);
                    value.peak_rss_measurement = ResourceMeasurement::unavailable(cause);
                }
                Self { observation, refresh_requested, stopped, worker: None }
            }
        }
    }

    /// Returns the latest published page without waiting for filesystem traversal or `ps`.
    pub(super) fn observe(&self) -> RunResourceObservation {
        if self.worker.as_ref().is_some_and(|worker| worker.is_finished()) {
            self.stopped.store(true, Ordering::Release);
            publish_interrupted(&self.observation, ResourceObservationCause::WorkerUnavailable);
        }
        let observation = self.observation.lock().map_or_else(
            |_| unavailable_observation(ResourceObservationCause::WorkerUnavailable),
            |value| *value,
        );
        self.refresh_requested.store(true, Ordering::Release);
        if let Some(worker) = &self.worker {
            worker.thread().unpark();
        }
        observation
    }
}

impl Drop for RunResourceProbe {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker.thread().unpark();
            // Teardown must not move a slow filesystem or platform observation back onto the
            // execution path. The worker owns cloned cancellation state and exits after its
            // current OS call returns; reap immediately only when it has already finished.
            if worker.is_finished() {
                let _ = worker.join();
            }
        }
    }
}

fn observe_resources(
    workspace_root: Option<PathBuf>,
    observation: Arc<Mutex<RunResourceObservation>>,
    refresh_requested: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
    cancelled: Arc<AtomicBool>,
) {
    let mut baseline_established = false;
    let mut peak_rss = None;
    loop {
        while !refresh_requested.swap(false, Ordering::AcqRel) {
            if stopped.load(Ordering::Acquire) || cancelled.load(Ordering::Acquire) {
                publish_cancelled(&observation);
                return;
            }
            thread::park();
        }
        if stopped.load(Ordering::Acquire) || cancelled.load(Ordering::Acquire) {
            publish_cancelled(&observation);
            return;
        }

        if workspace_root.is_none() {
            let memory = observe_memory(&mut peak_rss, &stopped, &cancelled);
            publish(
                &observation,
                ResourceMeasurement::unavailable(ResourceObservationCause::NotAuthorized),
                ResourceMeasurement::unavailable(ResourceObservationCause::NotAuthorized),
                memory,
            );
            continue;
        }

        let Some(workspace_root) = workspace_root.as_ref() else { continue };
        let memory = observe_memory(&mut peak_rss, &stopped, &cancelled);
        let mut scan = WorkspaceScan::new(workspace_root.to_owned());
        loop {
            if stopped.load(Ordering::Acquire) || cancelled.load(Ordering::Acquire) {
                publish_cancelled(&observation);
                return;
            }
            let (workspace, complete) = scan.advance_page();
            let growth = growth_measurement(workspace, baseline_established);
            publish(&observation, workspace, growth, memory);
            if complete {
                if !baseline_established && workspace.measured_value().is_some() {
                    baseline_established = true;
                    publish(
                        &observation,
                        workspace,
                        ResourceMeasurement::partial(
                            None,
                            workspace.coverage(),
                            ResourceObservationCause::BaselineEstablishedAfterStart,
                        ),
                        memory,
                    );
                }
                break;
            }
            thread::yield_now();
        }
    }
}

fn publish(
    shared: &Mutex<RunResourceObservation>,
    workspace: ResourceMeasurement,
    growth: ResourceMeasurement,
    peak_rss: ResourceMeasurement,
) {
    let Ok(mut observation) = shared.lock() else { return };
    if let Some(value) = workspace.measured_value() {
        observation.workspace = value;
    }
    if let Some(value) = growth.measured_value() {
        observation.growth = value;
    }
    if let Some(value) = peak_rss.value() {
        observation.peak_rss = observation.peak_rss.max(value);
    }
    observation.workspace_measurement = workspace;
    observation.growth_measurement = growth;
    observation.peak_rss_measurement = peak_rss;
}

fn publish_cancelled(shared: &Mutex<RunResourceObservation>) {
    publish_interrupted(shared, ResourceObservationCause::Cancelled);
}

fn publish_interrupted(
    shared: &Mutex<RunResourceObservation>,
    cause: ResourceObservationCause,
) {
    let Ok(mut observation) = shared.lock() else { return };
    observation.workspace_measurement = interrupted_measurement(
        observation.workspace_measurement,
        cause,
    );
    observation.growth_measurement = interrupted_measurement(
        observation.growth_measurement,
        cause,
    );
    observation.peak_rss_measurement =
        interrupted_measurement(observation.peak_rss_measurement, cause);
}

fn interrupted_measurement(
    previous: ResourceMeasurement,
    cause: ResourceObservationCause,
) -> ResourceMeasurement {
    if previous.cause().is_some_and(|prior| {
        !matches!(
            prior,
            ResourceObservationCause::NotObserved
                | ResourceObservationCause::ScanInProgress
        )
    }) {
        return previous;
    }
    match previous.status() {
        ResourceMeasurementStatus::Measured | ResourceMeasurementStatus::Unavailable => {
            ResourceMeasurement::unavailable_with_coverage(cause, previous.coverage())
        }
        ResourceMeasurementStatus::Partial => {
            ResourceMeasurement::partial(previous.value(), previous.coverage(), cause)
        }
    }
}

fn unavailable_observation(cause: ResourceObservationCause) -> RunResourceObservation {
    RunResourceObservation {
        workspace: 0,
        growth: 0,
        peak_rss: 0,
        workspace_measurement: ResourceMeasurement::unavailable(cause),
        growth_measurement: ResourceMeasurement::unavailable(cause),
        peak_rss_measurement: ResourceMeasurement::unavailable(cause),
    }
}

struct WorkspaceScan {
    pending: Option<(bool, PathBuf)>,
    parents: Vec<ReadDir>,
    current: Option<ReadDir>,
    entries_observed: u64,
    directories_completed: u64,
    items_unavailable: u64,
    bytes: Option<u64>,
    first_cause: Option<ResourceObservationCause>,
    root_failed: bool,
}

impl WorkspaceScan {
    fn new(root: PathBuf) -> Self {
        Self {
            pending: Some((true, root)),
            parents: Vec::new(),
            current: None,
            entries_observed: 0,
            directories_completed: 0,
            items_unavailable: 0,
            bytes: Some(0),
            first_cause: None,
            root_failed: false,
        }
    }

    fn advance_page(&mut self) -> (ResourceMeasurement, bool) {
        let mut transferred = 0;
        while transferred < WORKSPACE_TRANSFER_PAGE {
            if self.current.is_none() {
                if let Some((root, directory)) = self.pending.take() {
                    transferred += 1;
                    match fs::read_dir(&directory) {
                        Ok(entries) => self.current = Some(entries),
                        Err(error) => {
                            self.note_unavailable(io_cause(
                                ResourceObservationOperation::OpenDirectory,
                                &error,
                            ));
                            self.root_failed |= root;
                            self.current = self.parents.pop();
                            continue;
                        }
                    }
                } else if let Some(parent) = self.parents.pop() {
                    self.current = Some(parent);
                } else {
                    return (self.completed_measurement(), true);
                }
            }

            let next = self.current.as_mut().and_then(|entries| entries.next());
            match next {
                Some(Ok(entry)) => {
                    transferred += 1;
                    if !self.increment_entries() {
                        continue;
                    }
                    if entry.file_name() == ".git" {
                        continue;
                    }
                    match fs::symlink_metadata(entry.path()) {
                        Ok(metadata) if metadata.file_type().is_dir() => {
                            if let Some(parent) = self.current.take() {
                                self.parents.push(parent);
                            }
                            self.pending = Some((false, entry.path()));
                        }
                        Ok(metadata) if metadata.file_type().is_file() => {
                            if let Some(bytes) = self.bytes
                                && let Some(next) = bytes.checked_add(metadata.len())
                            {
                                self.bytes = Some(next);
                            } else {
                                self.bytes = None;
                                self.first_cause
                                    .get_or_insert(ResourceObservationCause::ArithmeticOverflow);
                            }
                        }
                        Ok(_) => {}
                        Err(error) => self.note_unavailable(io_cause(
                            ResourceObservationOperation::ReadMetadata,
                            &error,
                        )),
                    }
                }
                Some(Err(error)) => {
                    transferred += 1;
                    self.note_unavailable(io_cause(
                        ResourceObservationOperation::ReadDirectoryEntry,
                        &error,
                    ));
                }
                None => {
                    self.current = self.parents.pop();
                    if let Some(value) = self.directories_completed.checked_add(1) {
                        self.directories_completed = value;
                    } else {
                        self.first_cause
                            .get_or_insert(ResourceObservationCause::ArithmeticOverflow);
                    }
                }
            }
        }
        (
            ResourceMeasurement::partial(
                self.bytes,
                self.coverage(),
                self.first_cause.unwrap_or(ResourceObservationCause::ScanInProgress),
            ),
            false,
        )
    }

    fn increment_entries(&mut self) -> bool {
        if let Some(value) = self.entries_observed.checked_add(1) {
            self.entries_observed = value;
            true
        } else {
            self.bytes = None;
            self.first_cause.get_or_insert(ResourceObservationCause::ArithmeticOverflow);
            false
        }
    }

    fn note_unavailable(&mut self, cause: ResourceObservationCause) {
        if let Some(value) = self.items_unavailable.checked_add(1) {
            self.items_unavailable = value;
        } else {
            self.bytes = None;
            self.first_cause.get_or_insert(ResourceObservationCause::ArithmeticOverflow);
            return;
        }
        self.first_cause.get_or_insert(cause);
    }

    fn coverage(&self) -> ResourceObservationCoverage {
        ResourceObservationCoverage::new(
            self.entries_observed,
            self.directories_completed,
            self.items_unavailable,
            self.parents.len() as u64
                + u64::from(self.pending.is_some())
                + u64::from(self.current.is_some()),
        )
    }

    fn completed_measurement(&self) -> ResourceMeasurement {
        let coverage = self.coverage();
        let Some(bytes) = self.bytes else {
            return ResourceMeasurement::unavailable_with_coverage(
                self.first_cause.unwrap_or(ResourceObservationCause::ArithmeticOverflow),
                coverage,
            );
        };
        if self.root_failed {
            return ResourceMeasurement::unavailable_with_coverage(
                self.first_cause.unwrap_or(ResourceObservationCause::WorkerUnavailable),
                coverage,
            );
        }
        self.first_cause.map_or_else(
            || ResourceMeasurement::measured(bytes, coverage),
            |cause| ResourceMeasurement::partial(Some(bytes), coverage, cause),
        )
    }
}

fn growth_measurement(
    workspace: ResourceMeasurement,
    baseline_established: bool,
) -> ResourceMeasurement {
    if !baseline_established {
        return workspace.cause().map_or_else(
            || ResourceMeasurement::unavailable(ResourceObservationCause::NotObserved),
            |cause| ResourceMeasurement::partial(None, workspace.coverage(), cause),
        );
    }
    workspace.measured_value().map_or_else(
        || {
            ResourceMeasurement::partial(
                None,
                workspace.coverage(),
                workspace.cause().unwrap_or(ResourceObservationCause::ScanInProgress),
            )
        },
        |_| {
            ResourceMeasurement::partial(
                None,
                workspace.coverage(),
                ResourceObservationCause::BaselineEstablishedAfterStart,
            )
        },
    )
}

fn observe_memory(
    peak: &mut Option<u64>,
    stopped: &AtomicBool,
    cancelled: &AtomicBool,
) -> ResourceMeasurement {
    match resident_memory_bytes(stopped, cancelled) {
        Ok(value) => {
            let value = peak.map_or(value, |peak| peak.max(value));
            *peak = Some(value);
            ResourceMeasurement::measured(value, Default::default())
        }
        Err(cause) => peak.map_or_else(
            || ResourceMeasurement::unavailable(cause),
            |value| ResourceMeasurement::partial(Some(value), Default::default(), cause),
        ),
    }
}

#[cfg(target_os = "linux")]
fn resident_memory_bytes(
    _stopped: &AtomicBool,
    _cancelled: &AtomicBool,
) -> Result<u64, ResourceObservationCause> {
    let status = fs::read_to_string("/proc/self/status").map_err(|error| {
        io_cause(ResourceObservationOperation::ReadProcessMemory, &error)
    })?;
    linux_resident_memory_bytes(&status)
}

#[cfg(target_os = "linux")]
fn linux_resident_memory_bytes(status: &str) -> Result<u64, ResourceObservationCause> {
    let value = status
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))
        .and_then(|line| line.split_whitespace().next())
        .ok_or(ResourceObservationCause::InvalidPlatformData)?;
    parse_memory_text(value, 1024)
}

#[cfg(all(unix, not(target_os = "linux")))]
fn resident_memory_bytes(
    stopped: &AtomicBool,
    cancelled: &AtomicBool,
) -> Result<u64, ResourceObservationCause> {
    let mut child = Command::new(PROCESS_STATUS_COMMAND)
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| {
            io_cause(ResourceObservationOperation::StartProcessMemoryCommand, &error)
        })?;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                let output = child.wait_with_output().map_err(|error| {
                    io_cause(ResourceObservationOperation::ReadProcessMemory, &error)
                })?;
                return parse_memory_output(&output.stdout, 1024, output.status);
            }
            Ok(None) if stopped.load(Ordering::Acquire) || cancelled.load(Ordering::Acquire) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(ResourceObservationCause::Cancelled);
            }
            Ok(None) => thread::park_timeout(std::time::Duration::from_millis(10)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(io_cause(
                    ResourceObservationOperation::ReadProcessMemory,
                    &error,
                ));
            }
        }
    }
}

#[cfg(windows)]
fn resident_memory_bytes(
    _stopped: &AtomicBool,
    _cancelled: &AtomicBool,
) -> Result<u64, ResourceObservationCause> {
    peritus_process::current_process_resident_memory_bytes()
        .map_err(|error| io_cause(ResourceObservationOperation::ReadProcessMemory, &error))
}

#[cfg(all(unix, not(target_os = "linux")))]
fn parse_memory_output(
    output: &[u8],
    multiplier: u64,
    status: std::process::ExitStatus,
) -> Result<u64, ResourceObservationCause> {
    use std::os::unix::process::ExitStatusExt as _;

    if !status.success() {
        return Err(ResourceObservationCause::PlatformCommandFailed {
            exit_code: status.code(),
            signal: status.signal(),
        });
    }
    let text = std::str::from_utf8(output)
        .map_err(|_| ResourceObservationCause::InvalidPlatformData)?;
    parse_memory_text(text.trim(), multiplier)
}

#[cfg(unix)]
fn parse_memory_text(text: &str, multiplier: u64) -> Result<u64, ResourceObservationCause> {
    let value = text
        .parse::<u64>()
        .map_err(|_| ResourceObservationCause::InvalidPlatformData)?;
    value
        .checked_mul(multiplier)
        .ok_or(ResourceObservationCause::ArithmeticOverflow)
}

fn io_cause(
    operation: ResourceObservationOperation,
    error: &std::io::Error,
) -> ResourceObservationCause {
    ResourceObservationCause::Io {
        operation,
        kind: match error.kind() {
            std::io::ErrorKind::NotFound => ResourceIoErrorKind::NotFound,
            std::io::ErrorKind::PermissionDenied => ResourceIoErrorKind::PermissionDenied,
            std::io::ErrorKind::ConnectionRefused => ResourceIoErrorKind::ConnectionRefused,
            std::io::ErrorKind::ConnectionReset => ResourceIoErrorKind::ConnectionReset,
            std::io::ErrorKind::ConnectionAborted => ResourceIoErrorKind::ConnectionAborted,
            std::io::ErrorKind::NotConnected => ResourceIoErrorKind::NotConnected,
            std::io::ErrorKind::AddrInUse => ResourceIoErrorKind::AddressInUse,
            std::io::ErrorKind::AddrNotAvailable => ResourceIoErrorKind::AddressNotAvailable,
            std::io::ErrorKind::BrokenPipe => ResourceIoErrorKind::BrokenPipe,
            std::io::ErrorKind::AlreadyExists => ResourceIoErrorKind::AlreadyExists,
            std::io::ErrorKind::WouldBlock => ResourceIoErrorKind::WouldBlock,
            std::io::ErrorKind::InvalidInput => ResourceIoErrorKind::InvalidInput,
            std::io::ErrorKind::InvalidData => ResourceIoErrorKind::InvalidData,
            std::io::ErrorKind::TimedOut => ResourceIoErrorKind::TimedOut,
            std::io::ErrorKind::WriteZero => ResourceIoErrorKind::WriteZero,
            std::io::ErrorKind::Interrupted => ResourceIoErrorKind::Interrupted,
            std::io::ErrorKind::Unsupported => ResourceIoErrorKind::Unsupported,
            std::io::ErrorKind::UnexpectedEof => ResourceIoErrorKind::UnexpectedEof,
            std::io::ErrorKind::OutOfMemory => ResourceIoErrorKind::OutOfMemory,
            _ => ResourceIoErrorKind::Other,
        },
        raw_os_error: error.raw_os_error(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_growth_ignores_git_storage_and_does_not_follow_links() {
        let temporary = tempfile::tempdir().expect("resource root");
        fs::create_dir(temporary.path().join(".git")).expect("git directory");
        fs::write(temporary.path().join(".git/object"), vec![0_u8; 128]).expect("git object");
        fs::write(temporary.path().join("source.rs"), b"abc").expect("source");
        let probe = RunResourceProbe::new(temporary.path()).expect("resource probe");

        fs::write(temporary.path().join("artifact.bin"), vec![0_u8; 64]).expect("artifact");
        let observation = probe.observe().expect("resource observation");

        assert_eq!(observation.workspace, 67);
        assert_eq!(observation.growth, 64);
        assert!(observation.peak_rss > 0);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_memory_observation_uses_current_residency_instead_of_lifetime_high_water() {
        let status = "VmHWM:\t9000 kB\nVmRSS:\t1234 kB\n";

        assert_eq!(linux_resident_memory_bytes(status).expect("resident memory"), 1_263_616);
    }
}
