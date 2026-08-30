//! Owned native H2 controller process with bounded output and teardown.

use std::fs;
use std::io::{self, Read};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::QualificationError;

use super::NativeControllerLimits;
use super::native_error;
use super::process_tree::ProcessTree;

const POLL_INTERVAL: Duration = Duration::from_millis(20);
const MAX_DIAGNOSTIC_BYTES: usize = 16 * 1024;

pub(super) struct ProcessOutcome {
    pub(super) status: ExitStatus,
    pub(super) elapsed: Duration,
    pub(super) output_bytes: u64,
    pub(super) stderr: Vec<u8>,
}

#[derive(Clone, Copy)]
pub(super) struct ProcessRequest<'a> {
    pub(super) executable: &'a Path,
    pub(super) root: &'a Path,
    pub(super) package_root: &'a Path,
    pub(super) artifact_root: &'a Path,
    pub(super) request_path: &'a Path,
    pub(super) response_path: &'a Path,
    pub(super) cleanup_path: &'a Path,
    pub(super) subject_id: &'a str,
    pub(super) request_sha256: &'a str,
}

pub(super) fn execute(
    request: ProcessRequest<'_>,
    limits: NativeControllerLimits,
) -> Result<ProcessOutcome, QualificationError> {
    let mut command = Command::new(request.executable);
    command
        .arg("--request")
        .arg(request.request_path)
        .arg("--response")
        .arg(request.response_path)
        .arg("--cleanup-response")
        .arg(request.cleanup_path)
        .arg("--subject-root")
        .arg(request.root)
        .arg("--package-root")
        .arg(request.package_root)
        .arg("--artifact-root")
        .arg(request.artifact_root)
        .arg("--subject-id")
        .arg(request.subject_id)
        .arg("--request-sha256")
        .arg(request.request_sha256)
        .current_dir(request.root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear();
    configure_environment(&mut command, request.root);
    ProcessTree::configure(&mut command);
    let started = Instant::now();
    let mut child = OwnedChild::spawn(&mut command, limits.processes())?;
    let output = Arc::new(AtomicU64::new(0));
    let diagnostics = Arc::new(Mutex::new(Vec::new()));
    let stdout = drain(child.take_stdout()?, Arc::clone(&output), None);
    let stderr = drain(child.take_stderr()?, Arc::clone(&output), Some(Arc::clone(&diagnostics)));
    let status = loop {
        if output.load(Ordering::Acquire) > limits.output_bytes() {
            child.terminate()?;
            join(stdout)?;
            join(stderr)?;
            return Err(native_error(
                "execute native H2 controller",
                "controller output exceeded its hard byte limit",
            ));
        }
        if started.elapsed() > limits.duration() {
            child.terminate()?;
            join(stdout)?;
            join(stderr)?;
            return Err(native_error(
                "execute native H2 controller",
                "controller exceeded its monotonic deadline",
            ));
        }
        if let Some(status) = child.try_wait()? {
            break status;
        }
        thread::sleep(POLL_INTERVAL);
    };
    child.finish_tree()?;
    join(stdout)?;
    join(stderr)?;
    let stderr = diagnostics
        .lock()
        .map_err(|_| native_error("capture native H2 output", "diagnostic lock was poisoned"))?
        .clone();
    Ok(ProcessOutcome {
        status,
        elapsed: started.elapsed(),
        output_bytes: output.load(Ordering::Acquire),
        stderr,
    })
}

pub(super) fn read_document(
    path: &Path,
    maximum: u64,
    label: &'static str,
) -> Result<Vec<u8>, QualificationError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        native_error("read native H2 document", format!("{label} metadata: {error}"))
    })?;
    if !metadata.file_type().is_file() || metadata.len() == 0 || metadata.len() > maximum {
        return Err(native_error(
            "read native H2 document",
            format!("{label} must be a nonempty regular file within its byte bound"),
        ));
    }
    fs::read(path)
        .map_err(|error| native_error("read native H2 document", format!("{label} bytes: {error}")))
}

struct OwnedChild {
    child: Child,
    tree: ProcessTree,
    reaped: bool,
}

impl OwnedChild {
    fn spawn(command: &mut Command, maximum_processes: u32) -> Result<Self, QualificationError> {
        let mut child = command.spawn().map_err(|error| {
            native_error("launch native H2 controller", format!("spawn controller: {error}"))
        })?;
        let tree = match ProcessTree::attach(&child, maximum_processes) {
            Ok(tree) => tree,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };
        Ok(Self { child, tree, reaped: false })
    }

    fn take_stdout(&mut self) -> Result<impl Read + Send + 'static, QualificationError> {
        self.child.stdout.take().ok_or_else(|| {
            native_error("launch native H2 controller", "controller stdout was unavailable")
        })
    }

    fn take_stderr(&mut self) -> Result<impl Read + Send + 'static, QualificationError> {
        self.child.stderr.take().ok_or_else(|| {
            native_error("launch native H2 controller", "controller stderr was unavailable")
        })
    }

    fn try_wait(&mut self) -> Result<Option<ExitStatus>, QualificationError> {
        self.child.try_wait().map_err(|error| {
            native_error("wait for native H2 controller", format!("poll controller: {error}"))
        })
    }

    fn terminate(&mut self) -> Result<(), QualificationError> {
        if self.try_wait()?.is_none() {
            self.tree.terminate()?;
        }
        self.child.wait().map_err(|error| {
            native_error("terminate native H2 controller", format!("reap controller: {error}"))
        })?;
        self.reaped = true;
        Ok(())
    }

    fn finish_tree(&mut self) -> Result<(), QualificationError> {
        self.tree.finish()?;
        self.reaped = true;
        Ok(())
    }
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = self.tree.terminate();
            let _ = self.child.wait();
        }
    }
}

fn drain(
    reader: impl Read + Send + 'static,
    count: Arc<AtomicU64>,
    capture: Option<Arc<Mutex<Vec<u8>>>>,
) -> JoinHandle<io::Result<()>> {
    thread::spawn(move || {
        let mut reader = reader;
        let mut buffer = [0_u8; 16 * 1024];
        loop {
            let bytes = reader.read(&mut buffer)?;
            if bytes == 0 {
                return Ok(());
            }
            count.fetch_add(u64::try_from(bytes).unwrap_or(u64::MAX), Ordering::AcqRel);
            if let Some(capture) = &capture {
                let mut captured = capture.lock().map_err(|_| io::Error::other("capture lock"))?;
                let remaining = MAX_DIAGNOSTIC_BYTES.saturating_sub(captured.len());
                captured.extend_from_slice(&buffer[..bytes.min(remaining)]);
            }
        }
    })
}

fn join(handle: JoinHandle<io::Result<()>>) -> Result<(), QualificationError> {
    handle
        .join()
        .map_err(|_| native_error("drain native H2 output", "output reader panicked"))?
        .map_err(|error| native_error("drain native H2 output", format!("read pipe: {error}")))
}

fn configure_environment(command: &mut Command, root: &Path) {
    command
        .env("HOME", root)
        .env("USERPROFILE", root)
        .env("LOCALAPPDATA", root.join("local-app-data"))
        .env("APPDATA", root.join("app-data"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("TMPDIR", root.join("tmp"))
        .env("TEMP", root.join("tmp"))
        .env("TMP", root.join("tmp"));
    for name in ["PATH", "SYSTEMROOT", "WINDIR"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
}
