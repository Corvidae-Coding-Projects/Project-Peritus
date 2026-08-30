//! Owned native probe process with bounded output and cooperative termination.

use std::fs::{self, File};
use std::io::{self, Read};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::{CancellationToken, QualificationError, QualificationLimits};
use sha2::{Digest as _, Sha256};

use super::native_error;
use super::process_tree::ProcessTree;

const POLL_INTERVAL: Duration = Duration::from_millis(20);
const EXECUTABLE_BUSY_RETRIES: u8 = 4;

pub(super) struct ProcessOutcome {
    pub(super) status: ExitStatus,
    pub(super) elapsed: Duration,
    pub(super) output_bytes: u64,
}

#[derive(Clone, Copy)]
pub(super) struct ProcessRequest<'a> {
    pub(super) executable: &'a Path,
    pub(super) root: &'a Path,
    pub(super) request_path: &'a Path,
    pub(super) response_path: &'a Path,
    pub(super) artifact_root: &'a Path,
    pub(super) candidate_root: &'a Path,
    pub(super) subject_id: &'a str,
    pub(super) request_sha256: &'a str,
}

pub(super) fn execute(
    request: ProcessRequest<'_>,
    limits: QualificationLimits,
    cancellation: &CancellationToken,
) -> Result<ProcessOutcome, QualificationError> {
    let mut command = Command::new(request.executable);
    command
        .arg("--request")
        .arg(request.request_path)
        .arg("--response")
        .arg(request.response_path)
        .arg("--subject-root")
        .arg(request.root)
        .arg("--artifact-root")
        .arg(request.artifact_root)
        .arg("--subject-id")
        .arg(request.subject_id)
        .arg("--request-sha256")
        .arg(request.request_sha256)
        .arg("--candidate-root")
        .arg(request.candidate_root)
        .current_dir(request.root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear();
    preserve_runtime_environment(&mut command);
    ProcessTree::configure(&mut command);
    let started = Instant::now();
    let mut child = OwnedChild::spawn(&mut command, limits.max_processes())?;
    let output = Arc::new(AtomicU64::new(0));
    let stdout = drain(child.take_stdout()?, Arc::clone(&output));
    let stderr = drain(child.take_stderr()?, Arc::clone(&output));
    let deadline = Duration::from_millis(limits.max_duration_millis());
    let status = loop {
        if cancellation.is_cancelled() {
            child.terminate()?;
            join(stdout)?;
            join(stderr)?;
            return Err(native_error("execute native H0 probe", "campaign cancellation requested"));
        }
        if output.load(Ordering::Acquire) > limits.max_output_bytes() {
            child.terminate()?;
            join(stdout)?;
            join(stderr)?;
            return Err(native_error(
                "execute native H0 probe",
                "native output exceeded its hard byte limit",
            ));
        }
        if started.elapsed() > deadline {
            child.terminate()?;
            join(stdout)?;
            join(stderr)?;
            return Err(native_error(
                "execute native H0 probe",
                "native probe exceeded its monotonic deadline",
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
    Ok(ProcessOutcome {
        status,
        elapsed: started.elapsed(),
        output_bytes: output.load(Ordering::Acquire),
    })
}

pub(super) fn read_response(path: &Path, maximum: u64) -> Result<Vec<u8>, QualificationError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        native_error("read native H0 response", format!("response metadata: {error}"))
    })?;
    if !metadata.file_type().is_file() || metadata.len() == 0 || metadata.len() > maximum {
        return Err(native_error(
            "read native H0 response",
            "response must be a nonempty regular file within its byte bound",
        ));
    }
    fs::read(path).map_err(|error| {
        native_error("read native H0 response", format!("response bytes: {error}"))
    })
}

struct OwnedChild {
    child: Option<Child>,
    tree: ProcessTree,
    reaped: bool,
}

impl OwnedChild {
    fn spawn(command: &mut Command, maximum_processes: u32) -> Result<Self, QualificationError> {
        let mut retries = 0_u8;
        let child = loop {
            match command.spawn() {
                Ok(child) => break child,
                Err(error) if executable_is_busy(&error) && retries < EXECUTABLE_BUSY_RETRIES => {
                    retries += 1;
                    thread::sleep(Duration::from_millis(u64::from(retries) * 5));
                }
                Err(error) => {
                    return Err(native_error(
                        "launch native H0 probe",
                        format!("spawn executor: {error}"),
                    ));
                }
            }
        };
        let tree = ProcessTree::attach(&child, maximum_processes)?;
        Ok(Self { child: Some(child), tree, reaped: false })
    }

    fn take_stdout(&mut self) -> Result<impl Read + Send + 'static, QualificationError> {
        self.child.as_mut().and_then(|child| child.stdout.take()).ok_or_else(|| {
            native_error("launch native H0 probe", "executor stdout pipe was unavailable")
        })
    }

    fn take_stderr(&mut self) -> Result<impl Read + Send + 'static, QualificationError> {
        self.child.as_mut().and_then(|child| child.stderr.take()).ok_or_else(|| {
            native_error("launch native H0 probe", "executor stderr pipe was unavailable")
        })
    }

    fn try_wait(&mut self) -> Result<Option<ExitStatus>, QualificationError> {
        self.child
            .as_mut()
            .ok_or_else(|| native_error("wait for native H0 probe", "executor was released"))?
            .try_wait()
            .map_err(|error| {
                native_error("wait for native H0 probe", format!("poll executor: {error}"))
            })
    }

    fn terminate(&mut self) -> Result<(), QualificationError> {
        if self.try_wait()?.is_none() {
            self.tree.terminate()?;
        }
        self.child
            .as_mut()
            .ok_or_else(|| native_error("terminate native H0 probe", "executor was released"))?
            .wait()
            .map_err(|error| {
                native_error("terminate native H0 probe", format!("reap owned child: {error}"))
            })?;
        drop(self.child.take());
        self.reaped = true;
        Ok(())
    }

    fn finish_tree(&mut self) -> Result<(), QualificationError> {
        drop(self.child.take());
        self.reaped = true;
        self.tree.finish()
    }
}

#[cfg(unix)]
fn executable_is_busy(error: &io::Error) -> bool {
    error.raw_os_error() == Some(nix::libc::ETXTBSY)
}

#[cfg(not(unix))]
const fn executable_is_busy(_error: &io::Error) -> bool {
    false
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = self.tree.terminate();
            if let Some(child) = self.child.as_mut() {
                let _ = child.wait();
            }
        }
    }
}

fn drain(reader: impl Read + Send + 'static, count: Arc<AtomicU64>) -> JoinHandle<io::Result<()>> {
    thread::spawn(move || {
        let mut reader = reader;
        let mut buffer = [0_u8; 16 * 1024];
        loop {
            let bytes = reader.read(&mut buffer)?;
            if bytes == 0 {
                return Ok(());
            }
            count.fetch_add(u64::try_from(bytes).unwrap_or(u64::MAX), Ordering::AcqRel);
        }
    })
}

fn join(handle: JoinHandle<io::Result<()>>) -> Result<(), QualificationError> {
    handle
        .join()
        .map_err(|_| native_error("drain native H0 output", "output reader panicked"))?
        .map_err(|error| native_error("drain native H0 output", format!("read pipe: {error}")))
}

fn preserve_runtime_environment(command: &mut Command) {
    for name in ["PATH", "CARGO_HOME", "RUSTUP_HOME", "RUSTUP_TOOLCHAIN"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    #[cfg(windows)]
    for name in [
        "PATHEXT",
        "SYSTEMROOT",
        "WINDIR",
        "COMSPEC",
        "INCLUDE",
        "LIB",
        "LIBPATH",
        "ProgramFiles",
        "ProgramFiles(x86)",
        "VCINSTALLDIR",
        "VSCMD_ARG_HOST_ARCH",
        "VSCMD_ARG_TGT_ARCH",
        "VSTEL_MSBuildProjectFullPath",
        "VCToolsInstallDir",
        "VCToolsRedistDir",
        "VCToolsVersion",
        "WindowsSdkDir",
        "WindowsSDKVersion",
        "UniversalCRTSdkDir",
        "UCRTVersion",
        "VisualStudioVersion",
        "VSINSTALLDIR",
    ] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
}

pub(super) fn sha256_file(path: &Path) -> Result<peritus_types::Sha256Digest, QualificationError> {
    let mut file = File::open(path)
        .map_err(|error| native_error("digest native H0 file", format!("open file: {error}")))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let count = file.read(&mut buffer).map_err(|error| {
            native_error("digest native H0 file", format!("read file: {error}"))
        })?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(peritus_types::Sha256Digest::new(hasher.finalize().into()))
}
