//! Owned process, pipes, limits, and teardown for a native H1 controller.

use std::io::{self, BufRead as _, BufReader, Read, Write as _};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::{CancellationToken, SubjectError, SubjectErrorCode};

use super::process_tree::ProcessTree;
use super::{NativeControllerLimits, subject_error};

const POLL_INTERVAL: Duration = Duration::from_millis(20);

#[derive(Clone, Copy)]
pub(super) struct LaunchRequest<'a> {
    pub(super) executable: &'a Path,
    pub(super) subject_root: &'a Path,
    pub(super) artifact_root: &'a Path,
    pub(super) instance_id: &'a str,
    pub(super) subject_id: &'a str,
    pub(super) build_sha256: &'a str,
    pub(super) executor_sha256: &'a str,
}

pub(super) struct OwnedController {
    child: Option<Child>,
    stdin: ChildStdin,
    tree: ProcessTree,
    responses: Receiver<Result<Vec<u8>, SubjectError>>,
    stdout: Option<JoinHandle<()>>,
    stderr: Option<JoinHandle<io::Result<()>>>,
    output_bytes: Arc<AtomicU64>,
    reaped: bool,
}

impl OwnedController {
    pub(super) fn launch(
        request: LaunchRequest<'_>,
        limits: NativeControllerLimits,
    ) -> Result<Self, SubjectError> {
        let mut command = Command::new(request.executable);
        command
            .arg("--serve")
            .arg("--subject-root")
            .arg(request.subject_root)
            .arg("--artifact-root")
            .arg(request.artifact_root)
            .arg("--instance-id")
            .arg(request.instance_id)
            .arg("--subject-id")
            .arg(request.subject_id)
            .arg("--build-sha256")
            .arg(request.build_sha256)
            .arg("--executor-sha256")
            .arg(request.executor_sha256)
            .current_dir(request.subject_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env_clear();
        preserve_runtime_environment(&mut command);
        ProcessTree::configure(&mut command);
        Self::spawn(&mut command, limits)
    }

    fn spawn(command: &mut Command, limits: NativeControllerLimits) -> Result<Self, SubjectError> {
        let mut child = command
            .spawn()
            .map_err(|error| supervision(format!("spawn native controller: {error}"), true))?;
        let tree = match ProcessTree::attach(&child, limits.processes()) {
            Ok(tree) => tree,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| supervision("native controller stdin pipe is unavailable", false))?;
        let stdout_pipe = child
            .stdout
            .take()
            .ok_or_else(|| supervision("native controller stdout pipe is unavailable", false))?;
        let stderr_pipe = child
            .stderr
            .take()
            .ok_or_else(|| supervision("native controller stderr pipe is unavailable", false))?;
        let output_bytes = Arc::new(AtomicU64::new(0));
        let (response_sender, responses) = mpsc::channel();
        let stdout_count = Arc::clone(&output_bytes);
        let response_limit = limits.response_bytes();
        let stdout = thread::spawn(move || {
            read_responses(stdout_pipe, response_limit, &stdout_count, &response_sender);
        });
        let stderr_count = Arc::clone(&output_bytes);
        let stderr = thread::spawn(move || drain(stderr_pipe, &stderr_count));
        Ok(Self {
            child: Some(child),
            stdin,
            tree,
            responses,
            stdout: Some(stdout),
            stderr: Some(stderr),
            output_bytes,
            reaped: false,
        })
    }

    pub(super) fn exchange(
        &mut self,
        request: &[u8],
        abandoned: &AtomicBool,
        stop: &AtomicBool,
        cancellation: &CancellationToken,
        limits: NativeControllerLimits,
        cleanup: bool,
    ) -> Result<Vec<u8>, SubjectError> {
        self.stdin
            .write_all(request)
            .and_then(|()| self.stdin.flush())
            .map_err(|error| supervision(format!("write controller request: {error}"), true))?;
        let started = Instant::now();
        let output_start = self.output_bytes.load(Ordering::Acquire);
        let response = loop {
            if cancelled(abandoned, stop, cancellation) {
                self.terminate()?;
                return Err(supervision("controller operation was cancelled", false));
            }
            if started.elapsed() > limits.stage_duration() {
                self.terminate()?;
                return Err(supervision("controller stage exceeded its monotonic deadline", true));
            }
            if self.output_since(output_start) > limits.output_bytes() {
                self.terminate()?;
                return Err(supervision("controller stage exceeded its output byte limit", false));
            }
            match self.responses.recv_timeout(POLL_INTERVAL) {
                Ok(result) => break result?,
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    self.terminate()?;
                    return Err(supervision("controller response stream closed", true));
                }
            }
            if let Some(status) = self.try_wait()? {
                self.finish_tree()?;
                self.join_output()?;
                if cleanup {
                    if !status.success() {
                        return Err(supervision(
                            format!("controller cleanup exited with status {status}"),
                            false,
                        ));
                    }
                    match self.responses.try_recv() {
                        Ok(result) => break result?,
                        Err(TryRecvError::Empty | TryRecvError::Disconnected) => {}
                    }
                }
                return Err(supervision(
                    format!("controller exited before responding with status {status}"),
                    true,
                ));
            }
        };
        if self.output_since(output_start) > limits.output_bytes() {
            self.terminate()?;
            return Err(supervision("controller stage exceeded its output byte limit", false));
        }
        if cleanup {
            let status = self.wait_for_exit(
                started,
                limits.stage_duration(),
                abandoned,
                stop,
                cancellation,
            )?;
            self.finish_tree()?;
            self.join_output()?;
            if !status.success() {
                return Err(supervision(
                    format!("controller cleanup exited with status {status}"),
                    false,
                ));
            }
        }
        Ok(response)
    }

    fn output_since(&self, initial: u64) -> u64 {
        self.output_bytes.load(Ordering::Acquire).saturating_sub(initial)
    }

    fn wait_for_exit(
        &mut self,
        started: Instant,
        limit: Duration,
        abandoned: &AtomicBool,
        stop: &AtomicBool,
        cancellation: &CancellationToken,
    ) -> Result<ExitStatus, SubjectError> {
        loop {
            if let Some(status) = self.try_wait()? {
                return Ok(status);
            }
            if cancelled(abandoned, stop, cancellation) {
                self.terminate()?;
                return Err(supervision("controller cleanup was cancelled", false));
            }
            if started.elapsed() > limit {
                self.terminate()?;
                return Err(supervision("controller did not exit after cleanup", false));
            }
            thread::sleep(POLL_INTERVAL);
        }
    }

    fn try_wait(&mut self) -> Result<Option<ExitStatus>, SubjectError> {
        self.child
            .as_mut()
            .ok_or_else(|| supervision("native controller was released", false))?
            .try_wait()
            .map_err(|error| supervision(format!("poll native controller: {error}"), true))
    }

    fn terminate(&mut self) -> Result<(), SubjectError> {
        if self.try_wait()?.is_none() {
            self.tree.terminate()?;
        }
        self.child
            .as_mut()
            .ok_or_else(|| supervision("native controller was released", false))?
            .wait()
            .map_err(|error| supervision(format!("reap native controller: {error}"), false))?;
        drop(self.child.take());
        self.reaped = true;
        self.join_output()
    }

    fn finish_tree(&mut self) -> Result<(), SubjectError> {
        drop(self.child.take());
        self.reaped = true;
        self.tree.finish()
    }

    fn join_output(&mut self) -> Result<(), SubjectError> {
        if let Some(handle) = self.stdout.take() {
            handle.join().map_err(|_| supervision("controller stdout reader panicked", false))?;
        }
        if let Some(handle) = self.stderr.take() {
            handle
                .join()
                .map_err(|_| supervision("controller stderr reader panicked", false))?
                .map_err(|error| supervision(format!("read controller stderr: {error}"), false))?;
        }
        Ok(())
    }
}

impl Drop for OwnedController {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = self.tree.terminate();
            if let Some(child) = self.child.as_mut() {
                let _ = child.wait();
            }
            self.reaped = true;
        }
        let _ = self.join_output();
    }
}

fn read_responses(
    stdout: impl Read,
    maximum: u64,
    count: &AtomicU64,
    sender: &Sender<Result<Vec<u8>, SubjectError>>,
) {
    let mut reader = BufReader::new(stdout);
    loop {
        let mut line = Vec::new();
        let read = reader.by_ref().take(maximum.saturating_add(1)).read_until(b'\n', &mut line);
        let bytes = match read {
            Ok(0) => return,
            Ok(bytes) => bytes,
            Err(error) => {
                let _ = sender
                    .send(Err(supervision(format!("read controller response: {error}"), true)));
                return;
            }
        };
        count.fetch_add(u64::try_from(bytes).unwrap_or(u64::MAX), Ordering::AcqRel);
        if u64::try_from(line.len()).unwrap_or(u64::MAX) > maximum || !line.ends_with(b"\n") {
            let _ = sender.send(Err(supervision(
                "controller response exceeded its byte bound or lacked a newline",
                false,
            )));
            return;
        }
        line.pop();
        if line.is_empty() {
            let _ = sender.send(Err(supervision("controller returned an empty response", false)));
            return;
        }
        if sender.send(Ok(line)).is_err() {
            return;
        }
    }
}

fn drain(mut reader: impl Read, count: &AtomicU64) -> io::Result<()> {
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let bytes = reader.read(&mut buffer)?;
        if bytes == 0 {
            return Ok(());
        }
        count.fetch_add(u64::try_from(bytes).unwrap_or(u64::MAX), Ordering::AcqRel);
    }
}

fn cancelled(abandoned: &AtomicBool, stop: &AtomicBool, token: &CancellationToken) -> bool {
    abandoned.load(Ordering::Acquire) || stop.load(Ordering::Acquire) || token.is_cancelled()
}

fn supervision(detail: impl Into<String>, retryable: bool) -> SubjectError {
    subject_error(SubjectErrorCode::Supervision, detail, retryable)
}

fn preserve_runtime_environment(command: &mut Command) {
    for name in ["PATH", "SYSTEMROOT", "WINDIR"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
}
