//! Independent Linux owner-death watchdog for exact process groups.

use std::{
    io::Write as _,
    os::fd::OwnedFd,
    os::unix::net::UnixStream,
    os::unix::process::CommandExt as _,
    path::Path,
    process::{Child, Command, Stdio},
    time::Duration,
};

use crate::{ErrorCode, ProcessError, ProcessOperation, RecoveryClass};

use super::{PlatformProcess, ProcessTreeIdentity};

const DISARM: u8 = 1;
const REAP_POLL_INTERVAL: Duration = Duration::from_millis(5);

pub(super) fn attach(
    mut process: Box<dyn PlatformProcess>,
    executable: &Path,
) -> Result<Box<dyn PlatformProcess>, ProcessError> {
    let identity = process.identity();
    let watchdog = match CrashWatchdog::arm(executable, identity) {
        Ok(watchdog) => watchdog,
        Err(error) => {
            let _ = process.force_kill();
            return Err(error);
        }
    };
    Ok(Box::new(WatchedProcess { process, watchdog }))
}

struct WatchedProcess {
    process: Box<dyn PlatformProcess>,
    watchdog: CrashWatchdog,
}

impl PlatformProcess for WatchedProcess {
    fn identity(&self) -> ProcessTreeIdentity {
        self.process.identity()
    }

    fn take_input(&mut self) -> Option<super::ProcessInput> {
        self.process.take_input()
    }

    fn take_readers(&mut self) -> Vec<super::OutputReader> {
        self.process.take_readers()
    }

    fn try_wait(&mut self) -> Result<Option<super::PlatformExit>, ProcessError> {
        self.watchdog.ensure_armed()?;
        self.process.try_wait()
    }

    fn graceful_stop(&mut self, action: crate::GracefulAction) -> Result<(), ProcessError> {
        self.process.graceful_stop(action)
    }

    fn force_kill(&mut self) -> Result<(), ProcessError> {
        self.process.force_kill()
    }

    fn tree_quiescent(&mut self) -> Result<bool, ProcessError> {
        let quiescent = self.process.tree_quiescent()?;
        if quiescent {
            self.watchdog.disarm()?;
        } else {
            self.watchdog.ensure_armed()?;
        }
        Ok(quiescent)
    }

    fn process_count(&mut self) -> Result<Option<u64>, ProcessError> {
        self.watchdog.ensure_armed()?;
        self.process.process_count()
    }

    fn resize(&mut self, size: crate::TerminalSize) -> Result<(), ProcessError> {
        self.process.resize(size)
    }
}

struct CrashWatchdog {
    owner: Option<UnixStream>,
    child: Child,
}

impl CrashWatchdog {
    fn arm(executable: &Path, identity: ProcessTreeIdentity) -> Result<Self, ProcessError> {
        let root = identity.root_pid();
        let start = identity
            .start_token()
            .ok_or_else(|| watchdog_error("owned process birth identity is unavailable"))?;
        let group = identity
            .process_group()
            .ok_or_else(|| watchdog_error("owned process-group identity is unavailable"))?;
        if root == 0 || group != root || !identity.complete_containment() {
            return Err(watchdog_error("owned process identity cannot be guarded exactly"));
        }
        let (owner, observer) = UnixStream::pair()
            .map_err(|_| watchdog_error("process watchdog channel cannot be created"))?;
        let observer: OwnedFd = observer.into();
        let child = Command::new(executable)
            .arg("--process-watchdog-v1")
            .arg(root.to_string())
            .arg(start.to_string())
            .arg(group.to_string())
            .stdin(Stdio::from(observer))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
            .map_err(|_| watchdog_error("process crash watchdog cannot be started"))?;
        Ok(Self { owner: Some(owner), child })
    }

    fn ensure_armed(&mut self) -> Result<(), ProcessError> {
        if self.owner.is_none() {
            return Ok(());
        }
        match self.child.try_wait() {
            Ok(None) => Ok(()),
            Ok(Some(_)) => Err(watchdog_error("process crash watchdog exited while armed")),
            Err(_) => Err(watchdog_error("process crash watchdog cannot be observed")),
        }
    }

    fn disarm(&mut self) -> Result<(), ProcessError> {
        let Some(mut owner) = self.owner.take() else { return Ok(()) };
        if owner.write_all(&[DISARM]).is_err() {
            drop(owner);
            let _ = self.reap(false);
            return Err(watchdog_error("process crash watchdog cannot be disarmed"));
        }
        drop(owner);
        self.reap(true)
    }

    fn reap(&mut self, require_success: bool) -> Result<(), ProcessError> {
        loop {
            match self.child.try_wait() {
                Ok(Some(status)) if !require_success || status.success() => return Ok(()),
                Ok(Some(_)) => return Err(watchdog_error("process crash watchdog failed")),
                Ok(None) => {
                    std::thread::sleep(REAP_POLL_INTERVAL);
                }
                Err(_) => return Err(watchdog_error("process crash watchdog cannot be reaped")),
            }
        }
    }
}

impl Drop for CrashWatchdog {
    fn drop(&mut self) {
        self.owner.take();
        let _ = self.reap(false);
    }
}

const fn watchdog_error(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::ProcessTree,
        ProcessOperation::Spawn,
        RecoveryClass::CancelAndReap,
        detail,
    )
}
