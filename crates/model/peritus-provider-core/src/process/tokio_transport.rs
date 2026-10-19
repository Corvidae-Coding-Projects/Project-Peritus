//! Tokio-backed subprocess execution hidden behind Peritus-owned values.

use core::future::Future as _;
use std::future::poll_fn;
use std::pin::Pin;
use std::process::{ExitStatus, Stdio};
use std::task::Poll;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use tokio::time::Instant;

use super::{ProcessExit, ProcessOutput, ProcessRequest, ProcessTransport};
use crate::{BoxFuture, CancellationToken, ProviderCoreError};

/// Production subprocess transport backed by Tokio.
#[derive(Clone, Copy, Debug, Default)]
pub struct TokioProcessTransport;

impl ProcessTransport for TokioProcessTransport {
    fn run<'a>(
        &'a self,
        request: ProcessRequest,
        cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<ProcessOutput, ProviderCoreError>> {
        Box::pin(run(request, cancellation))
    }
}

async fn run(
    request: ProcessRequest,
    cancellation: &CancellationToken,
) -> Result<ProcessOutput, ProviderCoreError> {
    if cancellation.is_cancelled() {
        return Err(ProviderCoreError::cancelled("process_run"));
    }
    let limits = request.limits();
    let deadline = Instant::now() + limits.timeout();
    let mut child = spawn(&request)?;
    let stdin = write_stdin(
        &mut child,
        request.stdin(),
        cancellation,
        deadline,
        request.stdin_close_signal().is_some(),
    )
    .await?;
    let (stdout, stderr) = take_output(&mut child).await?;
    collect(child, stdout, stderr, stdin, limits, cancellation, deadline, &request).await
}

fn spawn(request: &ProcessRequest) -> Result<Child, ProviderCoreError> {
    let mut command = Command::new(request.executable().as_path());
    crate::process_containment::configure(&mut command);
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: provider transport uses pipes only.
    command
        .args(request.arguments())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(current_dir) = request.current_dir() {
        command.current_dir(current_dir);
    }
    for name in request.environment_removals() {
        command.env_remove(name.as_str());
    }
    for (name, value) in request.environment_overrides() {
        command.env(name.as_str(), value);
    }
    command.spawn().map_err(|_| {
        ProviderCoreError::connect("process_spawn", "owned subprocess could not be started")
    })
}

async fn write_stdin(
    child: &mut Child,
    input: &[u8],
    cancellation: &CancellationToken,
    deadline: Instant,
    keep_open: bool,
) -> Result<Option<ChildStdin>, ProviderCoreError> {
    let Some(mut stdin) = child.stdin.take() else {
        terminate(child).await;
        return Err(ProviderCoreError::transport(
            "process_spawn",
            "owned subprocess stdin was unavailable",
        ));
    };
    let write = async {
        stdin.write_all(input).await?;
        if keep_open { stdin.flush().await } else { stdin.shutdown().await }
    };
    let timed = tokio::time::timeout_at(deadline, write);
    let outcome = crate::cancellation::first(cancellation, timed).await;
    let result = match outcome {
        None => Err(ProviderCoreError::cancelled("process_stdin")),
        Some(Err(_)) => Err(timeout_error()),
        Some(Ok(Err(_))) => Err(ProviderCoreError::transport(
            "process_stdin",
            "owned subprocess stdin write failed",
        )),
        Some(Ok(Ok(()))) => Ok(()),
    };
    if result.is_err() {
        terminate(child).await;
    }
    result.map(|()| keep_open.then_some(stdin))
}

async fn take_output(child: &mut Child) -> Result<(ChildStdout, ChildStderr), ProviderCoreError> {
    let Some(stdout) = child.stdout.take() else {
        terminate(child).await;
        return Err(ProviderCoreError::transport(
            "process_spawn",
            "owned subprocess stdout was unavailable",
        ));
    };
    let Some(stderr) = child.stderr.take() else {
        terminate(child).await;
        return Err(ProviderCoreError::transport(
            "process_spawn",
            "owned subprocess stderr was unavailable",
        ));
    };
    Ok((stdout, stderr))
}

async fn collect(
    mut child: Child,
    stdout: ChildStdout,
    stderr: ChildStderr,
    stdin: Option<ChildStdin>,
    limits: super::ProcessLimits,
    cancellation: &CancellationToken,
    deadline: Instant,
    request: &ProcessRequest,
) -> Result<ProcessOutput, ProviderCoreError> {
    let operation = collect_output(&mut child, stdout, stderr, stdin, limits, request);
    let timed = tokio::time::timeout_at(deadline, operation);
    let outcome = crate::cancellation::first(cancellation, timed).await;
    let result = match outcome {
        None => Err(ProviderCoreError::cancelled("process_run")),
        Some(Err(_)) => Err(timeout_error()),
        Some(Ok(Err(error))) => Err(error),
        Some(Ok(Ok((status, stdout, stderr)))) => ProcessOutput::new(
            ProcessExit::new(status.success(), status.code()),
            stdout,
            stderr,
            limits,
        ),
    };
    if result.is_err() {
        terminate(&mut child).await;
    }
    result
}

async fn collect_output(
    child: &mut Child,
    stdout: ChildStdout,
    stderr: ChildStderr,
    mut stdin: Option<ChildStdin>,
    limits: super::ProcessLimits,
    request: &ProcessRequest,
) -> Result<(ExitStatus, Vec<u8>, Vec<u8>), ProviderCoreError> {
    let mut wait = Box::pin(child.wait());
    let mut stdout = Box::pin(read_bounded(
        stdout,
        limits.max_stdout_bytes(),
        "stdout",
        None,
        request.stdout_observer(),
    ));
    let mut stderr = Box::pin(read_bounded(
        stderr,
        limits.max_stderr_bytes(),
        "stderr",
        request.stderr_observer(),
        None,
    ));
    let mut status = None;
    let mut stdout_bytes = None;
    let mut stderr_bytes = None;
    let mut pending_input: Option<(Vec<u8>, usize)> = None;
    let mut input_bytes = request.stdin().len();
    poll_fn(|context| {
        if status.is_none() {
            match wait.as_mut().poll(context) {
                Poll::Ready(Ok(value)) => status = Some(value),
                Poll::Ready(Err(_)) => return Poll::Ready(Err(wait_error())),
                Poll::Pending => {}
            }
        }
        if stdout_bytes.is_none() {
            match stdout.as_mut().poll(context) {
                Poll::Ready(Ok(value)) => stdout_bytes = Some(value),
                Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                Poll::Pending => {}
            }
        }
        if stderr_bytes.is_none() {
            match stderr.as_mut().poll(context) {
                Poll::Ready(Ok(value)) => stderr_bytes = Some(value),
                Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                Poll::Pending => {}
            }
        }
        if pending_input.is_none()
            && let Some(messages) = request.stdin_messages()
        {
            let next = messages
                .lock()
                .map_err(|_| {
                    ProviderCoreError::transport("process_stdin", "protocol input queue poisoned")
                })?
                .pop_front();
            if let Some(next) = next {
                input_bytes = input_bytes.saturating_add(next.len());
                if input_bytes > limits.max_stdin_bytes {
                    return Poll::Ready(Err(ProviderCoreError::limit_exceeded(
                        "process_stdin",
                        "protocol input exceeded cumulative byte limit",
                    )));
                }
                pending_input = Some((next, 0));
            }
        }
        if let Some((bytes, offset)) = &mut pending_input {
            let input = stdin.as_mut().ok_or_else(|| {
                ProviderCoreError::transport("process_stdin", "protocol input already closed")
            })?;
            match Pin::new(input).poll_write(context, &bytes[*offset..]) {
                Poll::Ready(Ok(0)) | Poll::Ready(Err(_)) => {
                    return Poll::Ready(Err(ProviderCoreError::transport(
                        "process_stdin",
                        "protocol write failed",
                    )));
                }
                Poll::Ready(Ok(count)) => {
                    *offset += count;
                    if *offset == bytes.len() {
                        pending_input = None;
                    }
                    context.waker().wake_by_ref();
                }
                Poll::Pending => {}
            }
        }
        if pending_input.is_none()
            && request
                .stdin_messages()
                .is_none_or(|queue| queue.lock().is_ok_and(|queue| queue.is_empty()))
            && request
                .stdin_close_signal()
                .is_some_and(|signal| signal.load(std::sync::atomic::Ordering::Acquire))
        {
            drop(stdin.take());
        }
        match (status.take(), stdout_bytes.take(), stderr_bytes.take()) {
            (Some(status), Some(stdout), Some(stderr)) => Poll::Ready(Ok((status, stdout, stderr))),
            (pending_status, pending_stdout, pending_stderr) => {
                status = pending_status;
                stdout_bytes = pending_stdout;
                stderr_bytes = pending_stderr;
                Poll::Pending
            }
        }
    })
    .await
}

async fn read_bounded(
    mut input: impl AsyncRead + Unpin,
    limit: usize,
    operation: &'static str,
    observer: Option<fn(&[u8]) -> Option<ProviderCoreError>>,
    stdout_observer: Option<&(dyn Fn(&[u8]) + Send + Sync)>,
) -> Result<Vec<u8>, ProviderCoreError> {
    let mut output = Vec::new();
    let mut buffer = [0_u8; 8 * 1024];
    loop {
        let count = input.read(&mut buffer).await.map_err(|_| {
            ProviderCoreError::transport("process_output", "owned subprocess output read failed")
        })?;
        if count == 0 {
            return Ok(output);
        }
        let next = output.len().checked_add(count).ok_or_else(|| {
            ProviderCoreError::limit_exceeded(
                "process_output",
                "subprocess output length overflowed",
            )
        })?;
        if next > limit {
            return Err(ProviderCoreError::limit_exceeded(
                operation,
                "owned subprocess output exceeded its byte limit",
            ));
        }
        output.extend_from_slice(&buffer[..count]);
        if let Some(observe) = stdout_observer {
            observe(&output);
        }
        if let Some(error) = observer.and_then(|observe| observe(&output)) {
            return Err(error);
        }
    }
}

const fn wait_error() -> ProviderCoreError {
    ProviderCoreError::transport("process_wait", "owned subprocess terminal status was unavailable")
}

const fn timeout_error() -> ProviderCoreError {
    ProviderCoreError::transport(
        "process_timeout",
        "owned subprocess exceeded its wall-clock limit",
    )
}

async fn terminate(child: &mut Child) {
    let _ = child.start_kill();
    let _ = child.wait().await;
}
