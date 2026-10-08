//! Joined, cancellable child-process reads for candidate observation.

use std::{
    io::{Read as _, Write as _},
    process::{Command, ExitStatus, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use crate::{ProductRunnerError, ProductRunnerErrorKind};

const STDERR_BYTES: u64 = 64 * 1024;

#[derive(Clone)]
pub(crate) struct Cancellation {
    cancelled: Arc<AtomicBool>,
    provider: peritus_provider_core::CancellationToken,
}

impl Cancellation {
    pub(crate) fn new(
        cancelled: Arc<AtomicBool>,
        provider: peritus_provider_core::CancellationToken,
    ) -> Self {
        Self { cancelled, provider }
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire) || self.provider.is_cancelled()
    }

    fn inactive() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            provider: peritus_provider_core::CancellationToken::new(),
        }
    }

    pub(crate) fn check(&self, operation: &'static str) -> Result<(), ProductRunnerError> {
        if self.is_cancelled() { Err(cancelled(operation)) } else { Ok(()) }
    }
}

thread_local! {
    static ACTIVE: std::cell::RefCell<Option<Cancellation>> = const {
        std::cell::RefCell::new(None)
    };
}

pub(crate) fn with_cancellation<T>(
    cancellation: Cancellation,
    operation: impl FnOnce() -> Result<T, ProductRunnerError>,
) -> Result<T, ProductRunnerError> {
    struct Restore(Option<Cancellation>);
    impl Drop for Restore {
        fn drop(&mut self) {
            ACTIVE.with(|active| active.replace(self.0.take()));
        }
    }
    let previous = ACTIVE.with(|active| active.replace(Some(cancellation)));
    let _restore = Restore(previous);
    operation()
}

pub(crate) fn stream_current<W: Write + Send>(
    command: Command,
    input: Option<&[u8]>,
    output: &mut W,
    operation: &'static str,
) -> Result<Completed, ProductRunnerError> {
    let cancellation = ACTIVE.with(|active| active.borrow().clone()).unwrap_or_else(Cancellation::inactive);
    stream(command, input, output, &cancellation, operation)
}

pub(crate) struct Completed {
    pub(crate) status: ExitStatus,
    pub(crate) stderr: Vec<u8>,
}

/// Streams stdout while stderr and optional stdin are drained by joined owners. The caller's
/// cancellation is observed without imposing an elapsed deadline or buffering stdout in memory.
pub(crate) fn stream<W: Write + Send>(
    mut command: Command,
    input: Option<&[u8]>,
    output: &mut W,
    cancellation: &Cancellation,
    operation: &'static str,
) -> Result<Completed, ProductRunnerError> {
    if cancellation.is_cancelled() {
        return Err(cancelled(operation));
    }
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    if input.is_some() {
        command.stdin(Stdio::piped());
    } else {
        command.stdin(Stdio::null());
    }
    let mut child = command.spawn().map_err(|error| repository(operation, error))?;
    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            reap(&mut child);
            return Err(invariant(operation, "missing child stdout"));
        }
    };
    let stderr = match child.stderr.take() {
        Some(stderr) => stderr,
        None => {
            reap(&mut child);
            return Err(invariant(operation, "missing child stderr"));
        }
    };
    let stdin = if input.is_some() {
        match child.stdin.take() {
            Some(stdin) => Some(stdin),
            None => {
                reap(&mut child);
                return Err(invariant(operation, "missing child stdin"));
            }
        }
    } else {
        None
    };

    let mut was_cancelled = false;
    let (stdout_result, stderr_result, stdin_result, status) = std::thread::scope(|scope| {
        let stdout_owner = scope.spawn(|| {
            let mut stdout = stdout;
            std::io::copy(&mut stdout, output).map(|_| ())
        });
        let stderr_owner = scope.spawn(|| {
            let mut stderr = stderr;
            let mut bytes = Vec::new();
            let mut buffer = [0_u8; 8192];
            loop {
                let count = stderr.read(&mut buffer)?;
                if count == 0 { break; }
                let remaining = usize::try_from(STDERR_BYTES + 1)
                    .unwrap_or(usize::MAX)
                    .saturating_sub(bytes.len());
                bytes.extend_from_slice(&buffer[..count.min(remaining)]);
            }
            Ok::<_, std::io::Error>(bytes)
        });
        let stdin_owner = stdin.map(|mut stdin| {
            scope.spawn(move || stdin.write_all(input.unwrap_or_default()))
        });
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) if cancellation.is_cancelled() => {
                    was_cancelled = true;
                    let _ = child.kill();
                    break child.wait();
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(10)),
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err(error);
                }
            }
        };
        let stdin_result = stdin_owner.map(|owner| owner.join());
        (
            stdout_owner.join(),
            stderr_owner.join(),
            stdin_result,
            status,
        )
    });

    let status = status.map_err(|error| repository(operation, error))?;
    join_io(stdout_result, operation, "stdout owner")?;
    let stderr = join_value(stderr_result, operation, "stderr owner")?
        .map_err(|error| repository(operation, error))?;
    if let Some(result) = stdin_result {
        match join_value(result, operation, "stdin owner")? {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe && !status.success() => {}
            Err(error) => return Err(repository(operation, error)),
        }
    }
    if was_cancelled {
        return Err(cancelled(operation));
    }
    Ok(Completed { status, stderr })
}

fn reap(child: &mut std::process::Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn join_io(
    result: std::thread::Result<std::io::Result<()>>,
    operation: &'static str,
    owner: &'static str,
) -> Result<(), ProductRunnerError> {
    join_value(result, operation, owner)?.map_err(|error| repository(operation, error))
}

fn join_value<T>(
    result: std::thread::Result<T>,
    operation: &'static str,
    owner: &'static str,
) -> Result<T, ProductRunnerError> {
    result.map_err(|_| invariant(operation, owner))
}

fn cancelled(operation: &'static str) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::Cancelled,
        operation,
        "candidate observation was cancelled; the preceding durable evidence remains usable",
    )
}

fn repository(operation: &'static str, error: impl std::fmt::Display) -> ProductRunnerError {
    ProductRunnerError::new(ProductRunnerErrorKind::Repository, operation, error.to_string())
}

fn invariant(
    operation: &'static str,
    detail: impl std::fmt::Display,
) -> ProductRunnerError {
    ProductRunnerError::new(ProductRunnerErrorKind::InternalInvariant, operation, detail.to_string())
}
