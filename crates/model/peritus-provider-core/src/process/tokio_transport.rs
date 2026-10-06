//! Tokio-backed subprocess execution hidden behind Peritus-owned values.

mod journal;
mod output;

use core::future::Future as _;
use std::future::poll_fn;
use std::process::{ExitStatus, Stdio};
use std::task::Poll;

use tokio::io::AsyncWriteExt;
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use tokio::time::Instant;

use super::{ProcessExit, ProcessOutput, ProcessRequest, ProcessTransport};
use crate::{BoxFuture, CancellationToken, ProviderCoreError};
use journal::{JournalSender, JournalWriter};
use output::read_bounded;

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
    let journal = match request.stdout_journal() {
        Some(path) => Some(
            tokio::fs::OpenOptions::new().write(true).create_new(true).open(path).await.map_err(
                |_| {
                    ProviderCoreError::configuration(
                        "process_journal",
                        "owned subprocess journal could not be created",
                    )
                },
            )?,
        ),
        None => None,
    };
    let (journal_sender, mut journal) = JournalWriter::new(journal);
    let deadline = limits.timeout().map(|timeout| Instant::now() + timeout);
    let mut child = spawn(&request)?;
    let Some(stdin) = child.stdin.take() else {
        terminate(&mut child).await;
        return Err(ProviderCoreError::transport(
            "process_spawn",
            "owned subprocess stdin was unavailable",
        ));
    };
    let (stdout, stderr) = take_output(&mut child).await?;
    let operation = collect_output(
        &mut child,
        stdin,
        request.stdin(),
        (stdout, stderr),
        limits,
        (journal_sender, &mut journal),
    );
    let timed = async {
        match deadline {
            Some(deadline) => {
                tokio::time::timeout_at(deadline, operation).await.map_err(|_| timeout_error())?
            }
            None => operation.await,
        }
    };
    let result = match crate::cancellation::first(cancellation, timed).await {
        None => Err(ProviderCoreError::cancelled("process_run")),
        Some(Err(error)) => Err(error),
        Some(Ok((status, stdout, stderr))) => ProcessOutput::new(
            ProcessExit::new(status.success(), status.code()),
            stdout,
            stderr,
            limits,
        ),
    };
    if result.is_err() {
        terminate(&mut child).await;
    }
    // Cancellation drops pipe readers, not the accepted journal work. Keep its writer owned
    // outside the cancellable operation and observe persistence before releasing the lineage.
    journal.finish().await?;
    result
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
    command.spawn().map_err(|error| {
        ProviderCoreError::connect("process_spawn", "owned subprocess could not be started")
            .with_io_cause(&error)
    })
}

async fn write_stdin(mut stdin: ChildStdin, input: &[u8]) -> Result<(), ProviderCoreError> {
    async {
        stdin.write_all(input).await?;
        stdin.shutdown().await
    }
    .await
    .map_err(|_: std::io::Error| {
        ProviderCoreError::transport("process_stdin", "owned subprocess stdin write failed")
    })
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

async fn collect_output(
    child: &mut Child,
    stdin: ChildStdin,
    input: &[u8],
    output: (ChildStdout, ChildStderr),
    limits: super::ProcessLimits,
    journal: (Option<JournalSender>, &mut JournalWriter),
) -> Result<(ExitStatus, Vec<u8>, Vec<u8>), ProviderCoreError> {
    let (stdout, stderr) = output;
    let (journal_sender, journal) = journal;
    let mut wait = Box::pin(child.wait());
    let mut write = Box::pin(write_stdin(stdin, input));
    let mut written = false;
    let mut stdout =
        Box::pin(read_bounded(stdout, limits.max_stdout_bytes(), "stdout", journal_sender));
    let mut stderr = Box::pin(read_bounded(stderr, limits.max_stderr_bytes(), "stderr", None));
    let mut status = None;
    let mut stdout_bytes = None;
    let mut stderr_bytes = None;
    poll_fn(|context| {
        if !written {
            match write.as_mut().poll(context) {
                Poll::Ready(Ok(())) => written = true,
                Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                Poll::Pending => {}
            }
        }
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
        let persisted = match journal.poll(context) {
            Poll::Ready(Ok(())) => true,
            Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
            Poll::Pending => false,
        };
        match (status.take(), stdout_bytes.take(), stderr_bytes.take()) {
            (Some(status), Some(stdout), Some(stderr)) if written && persisted => {
                Poll::Ready(Ok((status, stdout, stderr)))
            }
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
