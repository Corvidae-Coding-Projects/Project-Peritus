//! Native `ConPTY` launch with suspended-process Job Object admission.
#![allow(unsafe_code, reason = "Windows handle ownership and ConPTY require documented FFI")]

#[path = "windows/launch.rs"]
mod launch;
#[path = "windows/native.rs"]
mod native;

use std::{fs::File, io::Write, thread::JoinHandle};
use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};

use crate::platform::{OutputReader, PlatformExit, PlatformProcess, ProcessTreeIdentity};
use crate::{GracefulAction, ProcessError, TerminalSize};
use native::{Console, Handle, Job};

pub(in crate::platform) use launch::launch;

struct WindowsPty {
    process: Handle,
    job: Job,
    console: Option<Console>,
    closing: Option<JoinHandle<()>>,
    identity: ProcessTreeIdentity,
    input: Option<File>,
    signal_input: Option<File>,
    readers: Vec<OutputReader>,
}

impl PlatformProcess for WindowsPty {
    fn identity(&self) -> ProcessTreeIdentity {
        self.identity
    }
    fn take_input(&mut self) -> Option<Box<dyn Write + Send>> {
        self.input.take().map(|file| Box::new(file) as Box<dyn Write + Send>)
    }
    fn take_readers(&mut self) -> Vec<OutputReader> {
        std::mem::take(&mut self.readers)
    }
    fn try_wait(&mut self) -> Result<Option<PlatformExit>, ProcessError> {
        // SAFETY: the process handle is live and zero timeout makes this observation nonblocking.
        match unsafe { WaitForSingleObject(self.process.raw(), 0) } {
            WAIT_TIMEOUT => return Ok(None),
            WAIT_OBJECT_0 => {}
            _ => return Err(error("ConPTY root exit cannot be observed")),
        }
        // Root exit does not let its descendants outlive the C2 execution.
        if self.job.active()? != 0 {
            self.job.terminate()?;
            return Ok(None);
        }
        self.signal_input.take();
        self.input.take();
        // ClosePseudoConsole may flush output. Close on an owned task while the supervisor keeps
        // draining its bounded output queue; publishing exit waits for this task to finish.
        if let Some(console) = self.console.take() {
            self.closing = Some(
                std::thread::Builder::new()
                    .name("peritus-conpty-close".into())
                    .spawn(move || drop(console))
                    .map_err(|source| {
                        error("ConPTY close task cannot start").with_source(source)
                    })?,
            );
        }
        if self.closing.as_ref().is_some_and(|task| !task.is_finished()) {
            return Ok(None);
        }
        if let Some(task) = self.closing.take() {
            task.join().map_err(|_| error("ConPTY close task panicked"))?;
        }
        let mut code = 0;
        // SAFETY: the live exited process handle and writable exit-code slot remain valid.
        if unsafe { GetExitCodeProcess(self.process.raw(), &raw mut code) } == 0 {
            return Err(error("ConPTY root exit code cannot be observed"));
        }
        Ok(Some(
            i32::try_from(code).map_or(PlatformExit::PlatformException(code), PlatformExit::Code),
        ))
    }
    fn graceful_stop(&mut self, action: GracefulAction) -> Result<(), ProcessError> {
        match action {
            GracefulAction::CloseInput => {
                self.input.take();
                self.signal_input.take();
                Ok(())
            }
            GracefulAction::Terminate => self.job.terminate(),
            GracefulAction::Interrupt => self
                .signal_input
                .as_mut()
                .ok_or_else(|| error("ConPTY input is closed"))?
                .write_all(&[3])
                .map_err(|source| error("ConPTY interrupt delivery failed").with_source(source)),
        }
    }
    fn force_kill(&mut self) -> Result<(), ProcessError> {
        self.input.take();
        self.signal_input.take();
        self.job.terminate()
    }
    fn tree_quiescent(&mut self) -> Result<bool, ProcessError> {
        Ok(self.job.active()? == 0)
    }
    fn process_count(&mut self) -> Result<Option<u64>, ProcessError> {
        self.job.active().map(Some)
    }
    fn resize(&mut self, size: TerminalSize) -> Result<(), ProcessError> {
        self.console.as_ref().ok_or_else(|| error("ConPTY is already closing"))?.resize(size)
    }
}

impl Drop for WindowsPty {
    fn drop(&mut self) {
        let _ = self.job.terminate();
        self.signal_input.take();
        self.input.take();
        self.readers.clear();
        self.console.take();
        if let Some(task) = self.closing.take() {
            // SpawnedOwner drops its output receiver first, releasing reader backpressure even
            // while unwinding. The close task remains owned until its result is observed.
            let _ = task.join();
        }
    }
}

const fn error(detail: &'static str) -> ProcessError {
    ProcessError::new(
        crate::ErrorCode::Pty,
        crate::ProcessOperation::Spawn,
        crate::RecoveryClass::CancelAndReap,
        detail,
    )
}
