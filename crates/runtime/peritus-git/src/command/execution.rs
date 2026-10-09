//! Joined subprocess-tree ownership for a single structured Git command.

#[cfg(windows)]
use process_wrap::std::JobObject;
#[cfg(unix)]
use process_wrap::std::ProcessSession;
use process_wrap::std::{ChildWrapper, CommandWrap};
use std::{
    io::{self, Read, Write},
    process::{Command, ExitStatus},
    thread,
    time::Duration,
};

use super::{
    CommandAccess, CommandOutput, GitCancellation, GitError, Operation, RecoveryClass, protocol,
};

#[cfg(test)]
mod tests;

pub(super) fn run(
    command: Command,
    input: Option<&[u8]>,
    cancellation: &GitCancellation,
    access: CommandAccess,
    operation: Operation,
) -> Result<CommandOutput, GitError> {
    if cancellation.is_cancelled() {
        return Err(cancelled(access, operation));
    }
    let mut command = CommandWrap::from(command);
    #[cfg(unix)]
    command.wrap(ProcessSession);
    #[cfg(windows)]
    command.wrap(JobObject);
    let child = command.spawn().map_err(|source| GitError::unavailable(operation, source))?;
    thread::scope(|scope| {
        // This guard drops before the scope joins its pipe workers on every error path.
        let mut child = OwnedChild { child, reaped: false };
        let stdout = child
            .child
            .stdout()
            .take()
            .ok_or_else(|| protocol(operation, "Git stdout pipe missing"))?;
        let stderr = child
            .child
            .stderr()
            .take()
            .ok_or_else(|| protocol(operation, "Git stderr pipe missing"))?;
        let stdout = scope.spawn(move || read_all(stdout));
        let stderr = scope.spawn(move || read_all(stderr));
        let stdin = match (input, child.child.stdin().take()) {
            (Some(bytes), Some(mut pipe)) => Some(scope.spawn(move || pipe.write_all(bytes))),
            (Some(_), None) => return Err(protocol(operation, "Git stdin pipe missing")),
            (None, _) => None,
        };
        let status = child.wait(cancellation).map_err(|source| {
            GitError::io(
                operation,
                RecoveryClass::Reconcile,
                "wait for owned Git process tree",
                source,
            )
        })?;
        let written = stdin.map(|worker| join(worker, operation, "write Git stdin")).transpose();
        let stdout = join(stdout, operation, "read Git stdout");
        let stderr = join(stderr, operation, "read Git stderr");
        // All pipe workers were joined before reporting cancellation or a pipe failure.
        if cancellation.is_cancelled() {
            return Err(cancelled(access, operation));
        }
        written?;
        Ok(CommandOutput { status, stdout: stdout?, stderr: stderr? })
    })
}

struct OwnedChild {
    child: Box<dyn ChildWrapper>,
    reaped: bool,
}

impl OwnedChild {
    fn wait(&mut self, cancellation: &GitCancellation) -> io::Result<ExitStatus> {
        loop {
            // Poll only the root. Polling a Windows job wrapper would consume its completion
            // notification, which belongs to the final whole-job wait below.
            let status = self.child.inner_mut().try_wait()?;
            if cancellation.is_cancelled() || status.is_some() {
                // No Git helper may outlive its root. Killing the owned group/job also closes
                // inherited pipe writers before scoped reader threads are joined.
                if let Err(error) = self.child.start_kill()
                    && !already_exited(&error)
                {
                    return Err(error);
                }
                let result = self.child.wait();
                self.reaped = result.is_ok();
                return result;
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}

fn already_exited(error: &io::Error) -> bool {
    #[cfg(unix)]
    if error.raw_os_error() == Some(libc::ESRCH) {
        return true;
    }
    error.kind() == io::ErrorKind::NotFound
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = self.child.start_kill();
            let _ = self.child.wait();
        }
    }
}

fn read_all(mut reader: impl Read) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn join<T>(
    worker: thread::ScopedJoinHandle<'_, io::Result<T>>,
    operation: Operation,
    detail: &'static str,
) -> Result<T, GitError> {
    worker
        .join()
        .map_err(|_| protocol(operation, "Git pipe worker panicked"))?
        .map_err(|source| GitError::io(operation, RecoveryClass::Retry, detail, source))
}

fn cancelled(access: CommandAccess, operation: Operation) -> GitError {
    GitError::new(
        crate::ErrorKind::Cancelled,
        operation,
        if access == CommandAccess::Write {
            RecoveryClass::Reconcile
        } else {
            RecoveryClass::Retry
        },
        "owned Git operation was cancelled; inspect any mutation before retrying",
    )
}
