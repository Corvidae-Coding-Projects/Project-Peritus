//! Cancellable native update helpers without an execution deadline.

#[cfg(not(windows))]
use std::process::Stdio;
use std::process::{Command, ExitStatus};

#[cfg(not(windows))]
use tokio::io::AsyncReadExt as _;

use crate::LauncherError;

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
                if root_exited(pid.as_raw())? {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }

            // The unreaped root pins its PID. Kill remaining group members before allowing
            // that identity to be reused, then reap the exact root through its Tokio owner.
            match nix::sys::signal::killpg(pid, nix::sys::signal::Signal::SIGKILL) {
                Ok(()) | Err(nix::errno::Errno::ESRCH) => {}
                Err(error) => return Err(std::io::Error::from_raw_os_error(error as i32)),
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
            // The owned kill-on-close job terminates any remaining descendants on drop.
            #[allow(
                unsafe_code,
                reason = "uniquely borrow only the root while retaining its job wrapper"
            )]
            // SAFETY: the root is neither moved nor replaced; the wrapper stays alive through await.
            let root = unsafe { self.0.inner_child_mut() };
            root.wait().await
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn captures_real_version_output() {
        let (status, output) =
            stdout(Command::new("sh").args(["-c", "printf 'peritus 1.2.3\\n'"]), "test output")
                .await
                .expect("output");
        assert!(status.success());
        assert_eq!(output, b"peritus 1.2.3\n");
    }

    #[tokio::test]
    async fn output_capture_reaps_descendants_that_keep_stdout_open() {
        let temporary = tempfile::tempdir().unwrap();
        let marker = temporary.path().join("must-not-finish");
        let (status, output) = stdout(
            Command::new("sh")
                .args(["-c", "(sleep 0.2; touch must-not-finish) & printf version; exit 0"])
                .current_dir(temporary.path()),
            "capture root exit",
        )
        .await
        .unwrap();
        assert!(status.success());
        assert_eq!(output, b"version");
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        assert!(!marker.exists());
    }

    #[tokio::test]
    async fn dropping_execution_cancels_owned_group() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let marker = temporary.path().join("finished");
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 0.2; touch \"$1\"", "test"]).arg(&marker);
        {
            let execution = status(&mut command, "cancel fixture");
            tokio::pin!(execution);
            tokio::select! {
                result = &mut execution => panic!("unexpected completion: {result:?}"),
                () = tokio::time::sleep(std::time::Duration::from_millis(20)) => {}
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        assert!(!marker.exists(), "cancelled descendant wrote its completion marker");
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;

    #[tokio::test]
    async fn cancelling_updater_kills_its_already_started_descendant() {
        let root = tempfile::tempdir().unwrap();
        let marker = root.path().join("descendant-finished");
        let ready = root.path().join("descendant-ready");
        let child_script = root.path().join("child.ps1");
        let parent_script = root.path().join("parent.ps1");
        let quote = |path: &std::path::Path| path.to_str().unwrap().replace('\'', "''");
        std::fs::write(
            &child_script,
            format!(
                "Set-Content -LiteralPath '{}' -Value ready\nStart-Sleep -Milliseconds 1000\nSet-Content -LiteralPath '{}' -Value finished",
                quote(&ready), quote(&marker)
            ),
        )
        .unwrap();
        std::fs::write(&parent_script, format!("Start-Process -FilePath powershell.exe -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', '\"{}\"') -NoNewWindow\nStart-Sleep -Seconds 30", quote(&child_script))).unwrap();
        let mut command = Command::new("powershell.exe");
        command.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"]).arg(&parent_script);
        {
            let execution = status(&mut command, "Windows cancel fixture");
            tokio::pin!(execution);
            let started = std::time::Instant::now();
            loop {
                tokio::select! {
                    result = &mut execution => panic!("unexpected early result: {result:?}"),
                    () = tokio::time::sleep(std::time::Duration::from_millis(10)) => {},
                }
                if ready.exists() {
                    break;
                }
                assert!(
                    started.elapsed() < std::time::Duration::from_secs(10),
                    "updater did not start"
                );
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        assert!(!marker.exists(), "cancelled updater descendant survived its Job Object");
    }
}

#[cfg(all(test, unix))]
mod root_exit_tests {
    #[tokio::test]
    async fn successful_root_exit_does_not_abandon_a_descendant() {
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("descendant-completed");
        let status = super::status(
            std::process::Command::new("sh")
                .args(["-c", "(sleep 0.2; printf survived > descendant-completed) & exit 0"])
                .current_dir(directory.path()),
            "root-exit fixture",
        )
        .await
        .unwrap();
        assert!(status.success());
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        assert!(!marker.exists());
    }
}
