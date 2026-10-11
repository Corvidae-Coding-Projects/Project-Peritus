//! Cancellable native update helpers without an execution deadline.

#[cfg(not(windows))]
use std::process::Stdio;
use std::process::{Command, ExitStatus};

#[cfg(not(windows))]
use tokio::io::AsyncReadExt as _;

use crate::LauncherError;

#[cfg(target_os = "macos")]
mod macos;

/// Owns a child until it has been reaped, including cancellation by dropping its future.
struct ChildOwner(
    #[cfg(not(windows))] tokio::process::Child,
    #[cfg(windows)] Box<dyn process_wrap::tokio::ChildWrapper>,
);

impl ChildOwner {
    async fn wait(&mut self) -> std::io::Result<ExitStatus> {
        #[cfg(unix)]
        {
            let pid =
                self.0.id().and_then(|pid| i32::try_from(pid).ok()).ok_or_else(|| {
                    std::io::Error::other("native updater has no live root identity")
                })?;
            let pid = nix::unistd::Pid::from_raw(pid);
            loop {
                if root_exited(pid.as_raw()).map_err(|error| {
                    std::io::Error::new(error.kind(), format!("observe updater root exit: {error}"))
                })? {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }

            // The unreaped root pins its PID. Kill remaining group members before allowing
            // that identity to be reused, then reap the exact root through its Tokio owner.
            match nix::sys::signal::killpg(pid, nix::sys::signal::Signal::SIGKILL) {
                Ok(()) | Err(nix::errno::Errno::ESRCH) => {}
                #[cfg(target_os = "macos")]
                Err(nix::errno::Errno::EPERM) if macos::only_group_member(pid.as_raw()) => {}
                Err(error) => {
                    let error = std::io::Error::from_raw_os_error(error as i32);
                    return Err(std::io::Error::new(
                        error.kind(),
                        format!("terminate updater process group: {error}"),
                    ));
                }
            }
            self.0.wait().await
        }
        #[cfg(not(any(unix, windows)))]
        {
            self.0.wait().await
        }
        #[cfg(windows)]
        {
            // Await the root without starting process-wrap's detached completion-port waiter.
            #[allow(
                unsafe_code,
                reason = "uniquely borrow only the root while retaining its job wrapper"
            )]
            // SAFETY: the root is neither moved nor replaced; the wrapper stays alive through await.
            let root = unsafe { self.0.inner_child_mut() };
            let status = root.wait().await?;
            // Terminate the owned Job explicitly before reporting success. process-wrap 9.1.0
            // empties its wrapper registry during spawn, so its KillOnDrop lookup cannot enable
            // the Job's kill-on-close flag even when the marker wrapper was configured.
            self.0.start_kill()?;
            Ok(status)
        }
    }
}

#[cfg(unix)]
#[allow(
    unsafe_code,
    reason = "waitid observes an owned child without reaping and returns initialized POSIX siginfo"
)]
fn root_exited(pid: i32) -> std::io::Result<bool> {
    let mut information = std::mem::MaybeUninit::<nix::libc::siginfo_t>::zeroed();
    // SAFETY: valid aligned writable siginfo storage and the exact child PID returned by spawn.
    let status = unsafe {
        nix::libc::waitid(
            nix::libc::P_PID,
            pid.cast_unsigned(),
            information.as_mut_ptr(),
            nix::libc::WEXITED | nix::libc::WNOWAIT | nix::libc::WNOHANG,
        )
    };
    if status != 0 {
        let error = std::io::Error::last_os_error();
        return if error.kind() == std::io::ErrorKind::Interrupted {
            Ok(false)
        } else {
            Err(error)
        };
    }
    // SAFETY: zero initialization plus successful waitid provides a valid siginfo record;
    // POSIX zero si_pid means no child has exited for this nonblocking observation.
    Ok(unsafe { information.assume_init().si_pid() } != 0)
}

impl Drop for ChildOwner {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Some(pid) = self.0.id().and_then(|pid| i32::try_from(pid).ok()) {
            let _ = nix::sys::signal::killpg(
                nix::unistd::Pid::from_raw(pid),
                nix::sys::signal::Signal::SIGKILL,
            );
        }
        #[cfg(windows)]
        // Dispatches to JobObjectChild::start_kill, terminating the whole owned tree.
        let _ = self.0.start_kill();
        // Tokio's kill-on-drop also arranges child reaping after future cancellation.
    }
}

fn spawn(command: &mut Command, operation: &'static str) -> Result<ChildOwner, LauncherError> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        command.process_group(0);
    }
    let mut command = tokio::process::Command::from(std::mem::replace(command, Command::new("")));
    #[cfg(not(windows))]
    return command
        .kill_on_drop(true)
        .spawn()
        .map(ChildOwner)
        .map_err(|error| LauncherError::Update(format!("{operation}: {error}")));
    #[cfg(windows)]
    {
        use process_wrap::tokio::{CommandWrap, JobObject, KillOnDrop};
        command.kill_on_drop(true);
        let mut wrapped = CommandWrap::from(command);
        wrapped.wrap(KillOnDrop).wrap(JobObject);
        wrapped
            .spawn()
            .map(ChildOwner)
            .map_err(|error| LauncherError::Update(format!("{operation}: {error}")))
    }
}

pub(super) async fn status(
    command: &mut Command,
    operation: &'static str,
) -> Result<ExitStatus, LauncherError> {
    let mut child = spawn(command, operation)?;
    tokio::select! {
        status = child.wait() => status.map_err(|error| LauncherError::Update(format!("{operation}: {error}"))),
        signal = tokio::signal::ctrl_c() => {
            signal.map_err(|error| LauncherError::Update(format!("listen for cancellation: {error}")))?;
            Err(LauncherError::Update(format!("{operation}: cancelled; inspect installation state before retrying")))
        }
    }
}

#[cfg(not(windows))]
pub(super) async fn stdout(
    command: &mut Command,
    operation: &'static str,
) -> Result<(ExitStatus, Vec<u8>), LauncherError> {
    command.stdout(Stdio::piped());
    let mut child = spawn(command, operation)?;
    let output = child
        .0
        .stdout
        .take()
        .ok_or_else(|| LauncherError::Update(format!("{operation}: missing stdout")))?;
    // Version replies have a bounded grammar; this is a protocol bound, not a run deadline.
    let mut output = output.take(65 * 1024);
    let mut bytes = Vec::new();
    let capture = async {
        output
            .read_to_end(&mut bytes)
            .await
            .map_err(|error| LauncherError::Update(format!("{operation}: {error}")))?;
        if bytes.len() > 64 * 1024 {
            return Err(LauncherError::Update(format!("{operation}: malformed version reply")));
        }
        Ok(())
    };
    let completion = async {
        child.wait().await.map_err(|error| LauncherError::Update(format!("{operation}: {error}")))
    };
    let status = tokio::select! {
        result = async { tokio::try_join!(capture, completion) } => result?.1,
        signal = tokio::signal::ctrl_c() => {
            signal.map_err(|error| LauncherError::Update(format!("listen for cancellation: {error}")))?;
            return Err(LauncherError::Update(format!("{operation}: cancelled")));
        }
    };
    Ok((status, bytes))
}
