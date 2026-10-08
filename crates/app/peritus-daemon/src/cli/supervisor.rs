//! Unbounded native service recovery with one isolated, reaped daemon child.

mod control;
mod process_owner;
#[cfg(test)]
mod tests;
#[cfg(windows)]
mod windows_stop;

use std::{
    ffi::{OsStr, OsString},
    fs,
    future::Future,
    io::{self, Write as _},
    path::{Path, PathBuf},
    process::{Command, ExitCode, ExitStatus, Stdio},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicU8, Ordering},
    },
    time::Duration,
};

use tokio::sync::Notify;

use control::{acquire_lease, control_paths, open_lock, publish_stop};
use process_owner::{OwnerClient, ProcessOwner};

const RESTART_DELAY: Duration = Duration::from_secs(5);
const CONTROL_POLL_INTERVAL: Duration = Duration::from_millis(100);
#[cfg(unix)]
const OWNER_PIPE_CANCEL_POLL_MILLIS: i32 = 50;
const RUNNING: u8 = 0;
const STOP_REQUESTED: u8 = 1;
const STOP_SOURCE_FAILED: u8 = 2;

struct StopIntent {
    disposition: AtomicU8,
    changed: Notify,
}

impl StopIntent {
    fn new() -> Self {
        Self { disposition: AtomicU8::new(RUNNING), changed: Notify::new() }
    }

    fn request(&self) {
        self.record(STOP_REQUESTED);
    }

    fn fail(&self) {
        self.record(STOP_SOURCE_FAILED);
    }

    fn record(&self, disposition: u8) {
        if self
            .disposition
            .compare_exchange(RUNNING, disposition, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            self.changed.notify_waiters();
        }
    }

    fn requested(&self) -> bool {
        self.disposition.load(Ordering::Acquire) != RUNNING
    }

    fn exit_code(&self) -> ExitCode {
        if self.disposition.load(Ordering::Acquire) == STOP_SOURCE_FAILED {
            ExitCode::FAILURE
        } else {
            ExitCode::SUCCESS
        }
    }

    async fn wait(&self) {
        loop {
            let changed = self.changed.notified();
            if self.requested() {
                return;
            }
            changed.await;
        }
    }
}

enum AttemptOutcome {
    Stopped(ExitCode),
    Failed(String),
}

pub(super) fn run(configuration: OsString) -> ExitCode {
    let control = match control_paths(&configuration) {
        Ok(control) => control,
        Err(error) => return report_io("derive daemon supervisor controls", error),
    };
    let lease = match acquire_lease(&control) {
        Ok(lease) => lease,
        Err(error) => return report_io("acquire daemon supervisor ownership", error),
    };
    let executable = match std::env::current_exe() {
        Ok(executable) => executable,
        Err(error) => return report_io("resolve supervised daemon executable", error),
    };
    let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(error) => {
            super::write_error(&format!("failed to construct daemon supervisor runtime: {error}"));
            return ExitCode::FAILURE;
        }
    };
    let owner_config = match crate::DaemonConfig::load(&configuration) {
        Ok(config) => config,
        Err(error) => {
            super::write_error(&format!("failed to load retained process owner config: {error}"));
            return ExitCode::FAILURE;
        }
    };
    let process_owner = Arc::new(ProcessOwner::new(owner_config, lease.token.as_bytes()));
    let stop = Arc::new(StopIntent::new());
    #[cfg(windows)]
    let native_stop = match windows_stop::Observer::start(
        Arc::clone(&stop),
        control.stop.clone(),
        lease.token.clone(),
    ) {
        Ok(observer) => observer,
        Err(error) => return report_io("start native daemon stop observer", error),
    };
    let signal_task = runtime.spawn(observe_stop_signal(
        Arc::clone(&stop),
        control.stop.clone(),
        lease.token.clone(),
    ));
    let marker_task =
        runtime.spawn(observe_stop_marker(Arc::clone(&stop), control.stop, lease.token.clone()));
    let result = runtime.block_on(supervise(
        Arc::clone(&stop),
        || {
            let executable = executable.clone();
            let configuration = configuration.clone();
            let owner_token = lease.token.clone();
            let stop = Arc::clone(&stop);
            let process_owner = Arc::clone(&process_owner);
            async move {
                server_attempt(
                    &executable,
                    &configuration,
                    OsStr::new(&owner_token),
                    stop,
                    process_owner,
                )
                .await
            }
        },
        || tokio::time::sleep(RESTART_DELAY),
        super::write_error,
    ));
    let owner_failures = process_owner.shutdown();
    signal_task.abort();
    marker_task.abort();
    #[cfg(windows)]
    let result = match native_stop.shutdown() {
        Ok(()) => result,
        Err(error) => {
            super::write_error(&format!("failed to stop native daemon observer: {error}"));
            ExitCode::FAILURE
        }
    };
    drop(lease);
    if owner_failures.is_empty() {
        result
    } else {
        for error in owner_failures {
            super::write_error(&format!("retained process shutdown failed: {error}"));
        }
        ExitCode::FAILURE
    }
}

pub(super) fn run_server(configuration: OsString, owner_token: OsString) -> ExitCode {
    let control = match control_paths(&configuration) {
        Ok(control) => control,
        Err(error) => return report_io("derive supervised daemon controls", error),
    };
    let owner_token = match owner_token.into_string() {
        Ok(owner_token) if !owner_token.is_empty() => owner_token,
        _ => {
            super::write_error("supervised daemon owner token must be nonempty UTF-8");
            return ExitCode::FAILURE;
        }
    };
    let owner = Arc::new(OwnerClient::inherited(owner_token.as_bytes()));
    super::server::run_supervised(configuration, &control.stop, &owner_token, owner)
}

pub(super) fn request_stop(configuration: OsString) -> ExitCode {
    let control = match control_paths(&configuration) {
        Ok(control) => control,
        Err(error) => return report_io("derive daemon supervisor controls", error),
    };
    let lock = match open_lock(&control.lock) {
        Ok(lock) => lock,
        Err(error) => return report_io("open daemon supervisor ownership", error),
    };
    loop {
        match fs4::FileExt::try_lock(&lock) {
            Ok(()) => {
                let _ = fs4::FileExt::unlock(&lock);
                return ExitCode::SUCCESS;
            }
            Err(fs4::TryLockError::WouldBlock) => {}
            Err(fs4::TryLockError::Error(error)) => {
                return report_io("inspect daemon supervisor ownership", error);
            }
        }
        match fs::read_to_string(&control.owner) {
            Ok(token) if !token.is_empty() => {
                if let Err(error) = publish_stop(&control.stop, &token) {
                    return report_io("publish daemon supervisor stop intent", error);
                }
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return report_io("read daemon supervisor ownership", error),
        }
        std::thread::sleep(CONTROL_POLL_INTERVAL);
    }
}

async fn observe_stop_signal(stop: Arc<StopIntent>, marker: PathBuf, token: String) {
    match tokio::signal::ctrl_c().await {
        Ok(()) => {
            retain_stop_request(&stop, &marker, &token, "retain daemon supervisor stop signal");
        }
        Err(error) => fail_stop_source(&stop, "observe daemon supervisor stop signal", &error),
    }
}

async fn observe_stop_marker(stop: Arc<StopIntent>, marker: PathBuf, token: String) {
    loop {
        match fs::read(&marker) {
            Ok(observed) if observed == token.as_bytes() => {
                stop.request();
                return;
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                fail_stop_source(&stop, "observe daemon supervisor stop marker", &error);
                return;
            }
        }
        tokio::time::sleep(CONTROL_POLL_INTERVAL).await;
    }
}

async fn server_attempt(
    executable: &Path,
    configuration: &OsStr,
    owner_token: &OsStr,
    stop: Arc<StopIntent>,
    process_owner: Arc<ProcessOwner>,
) -> AttemptOutcome {
    match serve_once(
        executable,
        configuration,
        owner_token,
        Arc::clone(&stop),
        process_owner,
    )
    .await
    {
        Ok(status) if stop.requested() => AttemptOutcome::Stopped(status_exit(status)),
        Ok(status) if status.success() => AttemptOutcome::Stopped(ExitCode::SUCCESS),
        Ok(status) if status.code() == Some(i32::from(super::server::SUPERVISED_UNCLEAN_EXIT)) => {
            AttemptOutcome::Stopped(ExitCode::FAILURE)
        }
        Ok(status) => AttemptOutcome::Failed(status.to_string()),
        Err(error) if stop.requested() => {
            super::write_error(&error.to_string());
            AttemptOutcome::Stopped(ExitCode::FAILURE)
        }
        Err(error) => AttemptOutcome::Failed(error.to_string()),
    }
}

async fn serve_once(
    executable: &Path,
    configuration: &OsStr,
    owner_token: &OsStr,
    stop: Arc<StopIntent>,
    process_owner: Arc<ProcessOwner>,
) -> io::Result<ExitStatus> {
    let mut child = serve_command(executable, configuration, owner_token).spawn()?;
    let Some(response_pipe) = child.stdin.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(io::Error::other("supervised daemon child has no private owner response pipe"));
    };
    let Some(mut request_pipe) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(io::Error::other("supervised daemon child has no private owner request pipe"));
    };
    let (mut broker_response, response_control) = match SharedChildInput::new(response_pipe) {
        Ok(response) => response,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    };
    let broker_owner = Arc::clone(&process_owner);
    let broker = tokio::task::spawn_blocking(move || {
        if let Err(error) = broker_response.activate() {
            broker_response.close();
            return Err(error);
        }
        let result = broker_owner.serve(&mut request_pipe, &mut broker_response);
        broker_response.close();
        result
    });
    let waiter = tokio::task::spawn_blocking(move || child.wait());
    tokio::pin!(waiter);
    let status = tokio::select! {
        result = &mut waiter => join_waiter(result),
        () = stop.wait() => {
            process_owner.begin_shutdown();
            let close = response_control.close();
            let status = join_waiter(waiter.await);
            match (close, status) {
                (Err(error), _) => Err(error),
                (Ok(()), result) => result,
            }
        }
    };
    let close_result = response_control.close();
    let broker_result = broker
        .await
        .map_err(|error| io::Error::other(format!("join retained process broker: {error}")))?;
    match (status, close_result, broker_result) {
        (Err(error), _, _) | (Ok(_), Err(error), _) | (Ok(_), Ok(()), Err(error)) => Err(error),
        (Ok(status), Ok(()), Ok(())) => Ok(status),
    }
}

fn serve_command(executable: &Path, configuration: &OsStr, owner_token: &OsStr) -> Command {
    let mut command = Command::new(executable);
    command
        .arg("--supervised-serve-v1")
        .arg("--config")
        .arg(configuration)
        .arg("--owner-token")
        .arg(owner_token)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped());
    command
}

struct SharedChildInput {
    state: Arc<ChildInputState>,
}

#[derive(Clone)]
struct ChildInputControl {
    state: Arc<ChildInputState>,
}

struct ChildInputState {
    pipe: Mutex<Option<std::process::ChildStdin>>,
    closed: AtomicBool,
    active_write: AtomicBool,
    write_wait: Mutex<()>,
    write_changed: Condvar,
    #[cfg(windows)]
    writer_thread: Mutex<Option<isize>>,
}

struct ActiveWrite<'a>(&'a ChildInputState);

impl Drop for ActiveWrite<'_> {
    fn drop(&mut self) {
        let _wait = self
            .0
            .write_wait
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.0.active_write.store(false, Ordering::Release);
        self.0.write_changed.notify_all();
    }
}

impl SharedChildInput {
    fn new(pipe: std::process::ChildStdin) -> io::Result<(Self, ChildInputControl)> {
        #[cfg(unix)]
        set_nonblocking(&pipe)?;
        let state = Arc::new(ChildInputState {
            pipe: Mutex::new(Some(pipe)),
            closed: AtomicBool::new(false),
            active_write: AtomicBool::new(false),
            write_wait: Mutex::new(()),
            write_changed: Condvar::new(),
            #[cfg(windows)]
            writer_thread: Mutex::new(None),
        });
        Ok((Self { state: Arc::clone(&state) }, ChildInputControl { state }))
    }

    #[cfg(not(windows))]
    fn activate(&mut self) -> io::Result<()> {
        Ok(())
    }

    #[cfg(windows)]
    #[allow(
        unsafe_code,
        reason = "OpenThread retains the exact broker thread for cancellable synchronous pipe I/O"
    )]
    fn activate(&mut self) -> io::Result<()> {
        use windows_sys::Win32::System::Threading::{
            GetCurrentThreadId, OpenThread, THREAD_TERMINATE,
        };

        // SAFETY: this broker thread remains alive until `serve_once` joins its task. The owned
        // handle is retained in ChildInputState and closed only after every shared reference drops.
        let handle = unsafe { OpenThread(THREAD_TERMINATE, 0, GetCurrentThreadId()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        *self
            .state
            .writer_thread
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(handle as isize);
        if self.state.closed.load(Ordering::Acquire) {
            self.state.cancel_windows_write()?;
        }
        Ok(())
    }

    fn close(&mut self) {
        let _ = ChildInputControl { state: Arc::clone(&self.state) }.close();
    }

    fn take_pipe(&self) -> io::Result<(std::process::ChildStdin, ActiveWrite<'_>)> {
        if self.state.active_write.swap(true, Ordering::AcqRel) {
            return Err(io::Error::other("daemon owner pipe has concurrent writers"));
        }
        let active = ActiveWrite(&self.state);
        if self.state.closed.load(Ordering::Acquire) {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "daemon owner pipe closed"));
        }
        let pipe = self
            .state
            .pipe
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
            .ok_or_else(|| io::Error::other("daemon owner pipe is unavailable"))?;
        Ok((pipe, active))
    }

    fn restore_pipe(&self, pipe: std::process::ChildStdin) {
        if self.state.closed.load(Ordering::Acquire) {
            drop(pipe);
        } else {
            *self
                .state
                .pipe
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(pipe);
        }
    }
}

impl io::Write for SharedChildInput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let (mut pipe, _active) = self.take_pipe()?;
        let result = loop {
            if self.state.closed.load(Ordering::Acquire) {
                break Err(io::Error::new(io::ErrorKind::BrokenPipe, "daemon owner pipe closed"));
            }
            match pipe.write(bytes) {
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                #[cfg(unix)]
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    wait_child_input_writable(&pipe, &self.state.closed)?;
                }
                result => break result,
            }
        };
        self.restore_pipe(pipe);
        result
    }

    fn flush(&mut self) -> io::Result<()> {
        let (mut pipe, _active) = self.take_pipe()?;
        let result = loop {
            if self.state.closed.load(Ordering::Acquire) {
                break Err(io::Error::new(io::ErrorKind::BrokenPipe, "daemon owner pipe closed"));
            }
            match pipe.flush() {
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                #[cfg(unix)]
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    wait_child_input_writable(&pipe, &self.state.closed)?;
                }
                result => break result,
            }
        };
        self.restore_pipe(pipe);
        result
    }
}

impl ChildInputControl {
    fn close(&self) -> io::Result<()> {
        self.state.closed.store(true, Ordering::Release);
        let mut wait = self
            .state
            .write_wait
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while self.state.active_write.load(Ordering::Acquire) {
            #[cfg(windows)]
            self.state.cancel_windows_write()?;
            wait = self
                .state
                .write_changed
                .wait(wait)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        drop(wait);
        self.state
            .pipe
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        Ok(())
    }
}

#[cfg(unix)]
#[allow(unsafe_code, reason = "poll waits for pipe readiness without spinning on backpressure")]
fn wait_child_input_writable(
    pipe: &std::process::ChildStdin,
    closed: &AtomicBool,
) -> io::Result<()> {
    use std::os::fd::AsRawFd;

    loop {
        if closed.load(Ordering::Acquire) {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "daemon owner pipe closed"));
        }
        let mut descriptor = libc::pollfd {
            fd: pipe.as_raw_fd(),
            events: libc::POLLOUT,
            revents: 0,
        };
        // SAFETY: `descriptor` points to one initialized pollfd for the duration of the call.
        let result = unsafe {
            libc::poll(
                &raw mut descriptor,
                1,
                OWNER_PIPE_CANCEL_POLL_MILLIS,
            )
        };
        match result {
            value if value > 0 && descriptor.revents & libc::POLLOUT != 0 => return Ok(()),
            value if value > 0 => {
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "daemon owner pipe is no longer writable",
                ));
            }
            0 => {}
            _ => {
                let error = io::Error::last_os_error();
                if error.kind() != io::ErrorKind::Interrupted {
                    return Err(error);
                }
            }
        }
    }
}

#[cfg(windows)]
impl ChildInputState {
    #[allow(
        unsafe_code,
        reason = "CancelSynchronousIo interrupts the exact retained broker response write"
    )]
    fn cancel_windows_write(&self) -> io::Result<()> {
        use windows_sys::Win32::{
            Foundation::ERROR_NOT_FOUND,
            System::IO::CancelSynchronousIo,
        };

        let Some(handle) = *self
            .writer_thread
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
        else {
            return Ok(());
        };
        // SAFETY: `activate` opened this exact broker-thread handle and ChildInputState retains it.
        if unsafe { CancelSynchronousIo(handle as _) } != 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        if error
            .raw_os_error()
            .and_then(|code| u32::try_from(code).ok())
            == Some(ERROR_NOT_FOUND)
        {
            return Ok(());
        }
        Err(error)
    }
}

#[cfg(windows)]
impl Drop for ChildInputState {
    #[allow(
        unsafe_code,
        reason = "CloseHandle releases the broker thread handle owned by ChildInputState"
    )]
    fn drop(&mut self) {
        use windows_sys::Win32::Foundation::CloseHandle;

        if let Some(handle) = self
            .writer_thread
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            // SAFETY: ChildInputState uniquely owns the handle returned by OpenThread.
            unsafe {
                CloseHandle(handle as _);
            }
        }
    }
}

#[cfg(unix)]
#[allow(
    unsafe_code,
    reason = "fcntl makes the uniquely owned supervisor response pipe cancellation-aware"
)]
fn set_nonblocking(pipe: &std::process::ChildStdin) -> io::Result<()> {
    use std::os::fd::AsRawFd as _;

    let fd = pipe.as_raw_fd();
    // SAFETY: ChildStdin retains ownership of `fd` across both flag operations.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags == -1 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: F_SETFL changes descriptor status flags without transferring ownership.
    if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn join_waiter(
    result: Result<io::Result<ExitStatus>, tokio::task::JoinError>,
) -> io::Result<ExitStatus> {
    result.map_err(|error| io::Error::other(format!("join supervised daemon waiter: {error}")))?
}

fn retain_stop_request(stop: &StopIntent, marker: &Path, token: &str, operation: &str) {
    match publish_stop(marker, token) {
        Ok(()) => stop.request(),
        Err(error) => fail_stop_source(stop, operation, &error),
    }
}

fn fail_stop_source(stop: &StopIntent, operation: &str, error: &dyn std::fmt::Display) {
    super::write_error(&format!("failed to {operation}: {error}"));
    stop.fail();
}

fn status_exit(status: ExitStatus) -> ExitCode {
    if status.success() { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

async fn supervise<Attempt, AttemptFuture, Delay, DelayFuture, Report>(
    stop: Arc<StopIntent>,
    mut attempt: Attempt,
    mut delay: Delay,
    mut report: Report,
) -> ExitCode
where
    Attempt: FnMut() -> AttemptFuture,
    AttemptFuture: Future<Output = AttemptOutcome>,
    Delay: FnMut() -> DelayFuture,
    DelayFuture: Future<Output = ()>,
    Report: FnMut(&str),
{
    loop {
        if stop.requested() {
            return stop.exit_code();
        }
        match attempt().await {
            AttemptOutcome::Stopped(exit) => return exit,
            AttemptOutcome::Failed(error) => report(&format!(
                "daemon service attempt failed ({error}); retrying the same configuration"
            )),
        }
        if stop.requested() {
            return stop.exit_code();
        }
        tokio::select! {
            () = stop.wait() => return stop.exit_code(),
            () = delay() => {}
        }
    }
}

fn report_io(operation: &str, error: io::Error) -> ExitCode {
    super::write_error(&format!("failed to {operation}: {error}"));
    ExitCode::FAILURE
}
