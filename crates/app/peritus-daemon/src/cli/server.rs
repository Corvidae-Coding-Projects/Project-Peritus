//! Tokio runtime ownership for the long-lived daemon command.

use std::{ffi::OsString, path::Path, process::ExitCode, sync::Arc, time::Duration};

use peritus_app_protocol::ShutdownCompletionDisposition;

use crate::{
    DaemonConfig, DaemonError, DaemonErrorCode, DaemonRecovery, DaemonRuntime, ShutdownOutcome,
    instance::HandoffOwner,
};

pub(super) const SUPERVISED_UNCLEAN_EXIT: u8 = 3;
const STOP_POLL_INTERVAL: Duration = Duration::from_millis(100);

pub(super) fn run(configuration: OsString) -> ExitCode {
    let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(error) => {
            super::write_error(&format!("failed to construct daemon runtime: {error}"));
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(serve(configuration, None, None)) {
        Ok(ServeCompletion::Shutdown(outcome)) => shutdown_exit(&outcome),
        Ok(ServeCompletion::ShutdownFailed(error)) => {
            super::write_error(&format!("daemon shutdown failed: {error}"));
            ExitCode::FAILURE
        }
        Ok(ServeCompletion::StoppedBeforeStart) => ExitCode::SUCCESS,
        Ok(ServeCompletion::WaitFailed { wait, shutdown }) => {
            report_wait_failure(wait, shutdown);
            ExitCode::FAILURE
        }
        Err(error) => {
            super::write_error(&error.to_string());
            ExitCode::FAILURE
        }
    }
}

pub(super) fn run_supervised(
    configuration: OsString,
    stop_marker: &Path,
    owner_token: &str,
    process_owner: Arc<dyn peritus_process::RetainedProcessTransport>,
) -> ExitCode {
    let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(error) => {
            super::write_error(&format!("failed to construct supervised daemon runtime: {error}"));
            return ExitCode::FAILURE;
        }
    };
    match runtime
        .block_on(serve(
            configuration,
            Some(SupervisorStop { marker: stop_marker, owner_token }),
            Some(process_owner),
        ))
    {
        Ok(ServeCompletion::StoppedBeforeStart) => ExitCode::SUCCESS,
        Ok(ServeCompletion::Shutdown(outcome)) => {
            let exit = shutdown_exit(&outcome);
            if exit == ExitCode::SUCCESS { exit } else { ExitCode::from(SUPERVISED_UNCLEAN_EXIT) }
        }
        Ok(ServeCompletion::ShutdownFailed(error)) => {
            super::write_error(&format!("daemon shutdown failed: {error}"));
            ExitCode::from(SUPERVISED_UNCLEAN_EXIT)
        }
        Ok(ServeCompletion::WaitFailed { wait, shutdown }) => {
            report_wait_failure(wait, shutdown);
            ExitCode::from(SUPERVISED_UNCLEAN_EXIT)
        }
        Err(error) => {
            super::write_error(&error.to_string());
            ExitCode::FAILURE
        }
    }
}

enum ServeCompletion {
    StoppedBeforeStart,
    Shutdown(ShutdownOutcome),
    ShutdownFailed(DaemonError),
    WaitFailed { wait: DaemonError, shutdown: Result<ShutdownOutcome, DaemonError> },
}

#[derive(Clone, Copy)]
struct SupervisorStop<'a> {
    marker: &'a Path,
    owner_token: &'a str,
}

async fn serve(
    configuration: OsString,
    supervisor_stop: Option<SupervisorStop<'_>>,
    process_owner: Option<Arc<dyn peritus_process::RetainedProcessTransport>>,
) -> Result<ServeCompletion, DaemonError> {
    if let Some(supervisor_stop) = supervisor_stop
        && supervisor_stop_requested(supervisor_stop)?
    {
        return Ok(ServeCompletion::StoppedBeforeStart);
    }
    let config = with_installed_process_watchdog(DaemonConfig::load(configuration)?)?;
    let handoff = HandoffOwner::current(&config)?;
    let startup_cancellation = peritus_journal::JournalCancellation::new();
    let startup = DaemonRuntime::start_cancellable_with_process_owner(
        config,
        startup_cancellation.clone(),
        process_owner,
    );
    let host_stop = wait_for_host_stop(supervisor_stop);
    tokio::pin!(startup);
    tokio::pin!(host_stop);
    let (runtime, startup_wait) = tokio::select! {
        // Poll the lazy stop future first so signal registration precedes the first blocking
        // startup owner. Pinning alone does not install a Tokio signal listener.
        biased;
        wait = &mut host_stop => {
            startup_cancellation.cancel();
            (startup.await?, Some(wait))
        },
        result = &mut startup => (result?, None),
    };
    let Some(mut runtime) = runtime else {
        return match startup_wait {
            Some(Ok(())) | None => Ok(ServeCompletion::StoppedBeforeStart),
            Some(Err(error)) => Err(error),
        };
    };
    let wait = if let Some(wait) = startup_wait {
        wait
    } else {
        tokio::select! {
            result = &mut host_stop => result,
            result = runtime.wait_for_runtime_shutdown() => result,
            result = handoff.wait() => result,
        }
    };
    let wait_succeeded = wait.is_ok();
    let shutdown = runtime.shutdown().await;
    let handoff_completion = handoff.complete(wait_succeeded, &shutdown);
    drop(runtime);
    if let Err(error) = handoff_completion {
        return Ok(ServeCompletion::ShutdownFailed(error));
    }
    match wait {
        Ok(()) => Ok(match shutdown {
            Ok(outcome) => ServeCompletion::Shutdown(outcome),
            Err(error) => ServeCompletion::ShutdownFailed(error),
        }),
        Err(wait) => Ok(ServeCompletion::WaitFailed { wait, shutdown }),
    }
}

async fn wait_for_host_stop(
    supervisor_stop: Option<SupervisorStop<'_>>,
) -> Result<(), DaemonError> {
    if let Some(supervisor_stop) = supervisor_stop {
        tokio::select! {
            result = DaemonRuntime::wait_for_host_shutdown_signal() => result,
            result = wait_for_supervisor_stop(supervisor_stop) => result,
        }
    } else {
        DaemonRuntime::wait_for_host_shutdown_signal().await
    }
}

fn supervisor_stop_requested(stop: SupervisorStop<'_>) -> Result<bool, DaemonError> {
    marker_matches(stop).and_then(|marker| {
        if marker { Ok(true) } else { supervisor_pipe_requested().map_err(supervisor_stop_error) }
    })
}

fn marker_matches(stop: SupervisorStop<'_>) -> Result<bool, DaemonError> {
    match std::fs::read(stop.marker) {
        Ok(observed) => Ok(observed == stop.owner_token.as_bytes()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(supervisor_stop_error(error)),
    }
}

async fn wait_for_supervisor_stop(stop: SupervisorStop<'_>) -> Result<(), DaemonError> {
    loop {
        if supervisor_stop_requested(stop)? {
            return Ok(());
        }
        tokio::time::sleep(STOP_POLL_INTERVAL).await;
    }
}

#[cfg(unix)]
#[allow(
    unsafe_code,
    reason = "nonblocking poll of the inherited private stop pipe requires the libc boundary"
)]
fn supervisor_pipe_requested() -> std::io::Result<bool> {
    let mut descriptor =
        libc::pollfd { fd: libc::STDIN_FILENO, events: libc::POLLHUP, revents: 0 };
    // SAFETY: `descriptor` is live and uniquely borrowed for this nonblocking one-entry poll.
    let result = unsafe { libc::poll(&raw mut descriptor, 1, 0) };
    if result < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(result > 0
        && descriptor.revents & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) != 0)
}

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "nonblocking inspection of the inherited private stop pipe requires Win32 handles"
)]
fn supervisor_pipe_requested() -> std::io::Result<bool> {
    use std::ptr;

    use windows_sys::Win32::{
        Foundation::{ERROR_BROKEN_PIPE, INVALID_HANDLE_VALUE},
        System::{
            Console::{GetStdHandle, STD_INPUT_HANDLE},
            Pipes::PeekNamedPipe,
        },
    };

    // SAFETY: the process standard-input slot is read without transferring handle ownership.
    let input = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
    if input.is_null() || input == INVALID_HANDLE_VALUE {
        return Ok(true);
    }
    let mut available = 0;
    // SAFETY: `input` is the inherited pipe handle and `available` is a valid output pointer.
    let result = unsafe {
        PeekNamedPipe(
            input,
            ptr::null_mut(),
            0,
            ptr::null_mut(),
            &raw mut available,
            ptr::null_mut(),
        )
    };
    if result != 0 {
        return Ok(false);
    }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(ERROR_BROKEN_PIPE.cast_signed()) {
        Ok(true)
    } else {
        Err(error)
    }
}

#[cfg(not(any(unix, windows)))]
fn supervisor_pipe_requested() -> std::io::Result<bool> {
    Ok(false)
}

fn supervisor_stop_error(error: std::io::Error) -> DaemonError {
    DaemonError::with_source(
        DaemonErrorCode::Storage,
        DaemonRecovery::Operator,
        "observe daemon supervisor stop intent",
        "supervisor stop marker cannot be inspected",
        error,
    )
}

fn report_wait_failure(wait: DaemonError, shutdown: Result<ShutdownOutcome, DaemonError>) {
    super::write_error(&format!("daemon stop observation failed: {wait}"));
    match shutdown {
        Ok(outcome) => {
            let _ = shutdown_exit(&outcome);
        }
        Err(error) => super::write_error(&format!(
            "daemon shutdown after stop-observation failure also failed: {error}"
        )),
    }
}

fn shutdown_exit(outcome: &ShutdownOutcome) -> ExitCode {
    if outcome.disposition() == ShutdownCompletionDisposition::Clean {
        ExitCode::SUCCESS
    } else {
        super::write_error(&format!(
            "daemon shutdown was unclean: remaining={:?}, failures={:?}",
            outcome.remaining(),
            outcome.failures(),
        ));
        ExitCode::FAILURE
    }
}

#[cfg(target_os = "linux")]
fn with_installed_process_watchdog(config: DaemonConfig) -> Result<DaemonConfig, DaemonError> {
    let daemon = std::env::current_exe().map_err(|error| {
        DaemonError::with_source(
            DaemonErrorCode::RecoveryRequired,
            DaemonRecovery::Operator,
            "resolve process crash watchdog",
            "installed daemon executable cannot be resolved",
            error,
        )
    })?;
    Ok(config.with_process_crash_watchdog(daemon))
}

#[cfg(not(target_os = "linux"))]
const fn with_installed_process_watchdog(
    config: DaemonConfig,
) -> Result<DaemonConfig, DaemonError> {
    Ok(config)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{SupervisorStop, marker_matches};

    #[test]
    fn retained_stop_marker_must_match_the_current_supervisor_generation() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let marker = temporary.path().join("supervisor.stop");
        let stop = SupervisorStop { marker: &marker, owner_token: "current-generation" };

        assert!(!marker_matches(stop).expect("missing marker"));
        fs::write(&marker, b"stale-generation").expect("stale marker");
        assert!(!marker_matches(stop).expect("stale marker inspection"));
        fs::write(&marker, b"current-generation").expect("current marker");
        assert!(marker_matches(stop).expect("current marker inspection"));
    }
}
