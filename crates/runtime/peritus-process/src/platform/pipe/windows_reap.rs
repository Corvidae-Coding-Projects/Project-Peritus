//! Bounded Windows job-object completion ownership.

use std::{
    sync::mpsc::{Receiver, TryRecvError, sync_channel},
    thread::{self, JoinHandle},
};

use crate::ProcessError;

use super::{PipeProcess, tree_error};

pub(super) struct WindowsJobReap {
    pub(super) completion: Receiver<std::io::Result<std::process::ExitStatus>>,
    pub(super) task: JoinHandle<()>,
}

impl PipeProcess {
    pub(super) fn request_job_termination(
        &mut self,
        detail: &'static str,
    ) -> Result<(), ProcessError> {
        if self.termination_requested || self.job_reaped {
            return Ok(());
        }
        self.child
            .as_mut()
            .ok_or_else(|| tree_error("pipe process job handle is unavailable"))?
            .start_kill()
            .map_err(|_| tree_error(detail))?;
        self.termination_requested = true;
        Ok(())
    }

    pub(super) fn poll_job_reap(&mut self) -> Result<bool, ProcessError> {
        if self.job_reaped {
            return Ok(true);
        }
        if !self.termination_requested {
            return Ok(false);
        }
        if self.job_reap.is_none() {
            self.start_job_reap()?;
        }
        let Some(reap) = self.job_reap.as_ref() else {
            return Ok(false);
        };
        let result = match reap.completion.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return Ok(false),
            Err(TryRecvError::Disconnected) => {
                let reap = self
                    .job_reap
                    .take()
                    .ok_or_else(|| tree_error("job reap task is unavailable"))?;
                reap.task.join().map_err(|_| tree_error("Windows job reap task panicked"))?;
                return Err(tree_error("Windows job reap task disconnected"));
            }
        };
        let reap =
            self.job_reap.take().ok_or_else(|| tree_error("job reap task is unavailable"))?;
        reap.task.join().map_err(|_| tree_error("Windows job reap task panicked"))?;
        result.map_err(|_| tree_error("Windows job completion wait failed"))?;
        self.job_reaped = true;
        Ok(true)
    }

    fn start_job_reap(&mut self) -> Result<(), ProcessError> {
        let mut child = self
            .child
            .take()
            .ok_or_else(|| tree_error("pipe process job handle is unavailable"))?;
        let (completion, observation) = sync_channel(1);
        let task = thread::Builder::new()
            .name("peritus-windows-job-reap".to_owned())
            .spawn(move || {
                let _ = completion.send(child.wait());
            })
            .map_err(|_| tree_error("Windows job reap task cannot be started"))?;
        self.job_reap = Some(WindowsJobReap { completion: observation, task });
        Ok(())
    }
}
