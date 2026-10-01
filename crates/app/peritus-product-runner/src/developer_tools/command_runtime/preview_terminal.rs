//! A terminal observer keeps the C4 owner alive without moving or duplicating process ownership.

use super::CommandRuntime;
use crate::{PreviewLaunch, PreviewProcessState, ProductRunnerError, ProductRunnerErrorKind};
use peritus_process::{ExecutionPlan, ProcessControl, TerminalResult};

/// A live preview terminal lease. C4 remains the sole owner and publishes the final tool result.
pub struct PreviewTerminal {
    runtime: CommandRuntime,
    launch: PreviewLaunch,
    plan: ExecutionPlan,
    control: ProcessControl,
}

impl CommandRuntime {
    /// Obtains an observer for an exact currently active interactive preview.
    ///
    /// # Errors
    /// Rejects terminal, non-interactive, or mismatched process identities.
    pub fn preview_terminal(
        &self,
        launch: &PreviewLaunch,
    ) -> Result<PreviewTerminal, ProductRunnerError> {
        let state = self.inner.state.lock().map_err(|_| failure("runtime is poisoned"))?;
        let active =
            state.active.get(&launch.handle).ok_or_else(|| failure("preview is not live"))?;
        if !active.interactive || active.plan.identity().process_id() != launch.process_id() {
            return Err(failure("preview is not the exact interactive process"));
        }
        let lease = PreviewTerminal {
            runtime: self.clone(),
            launch: launch.clone(),
            plan: active.plan.clone(),
            control: active
                .control
                .clone()
                .ok_or_else(|| failure("process control unavailable"))?,
        };
        drop(state);
        Ok(lease)
    }
}

impl PreviewTerminal {
    /// Borrows the original checked execution plan, including its internal C4 identity.
    #[must_use]
    pub const fn plan(&self) -> &ExecutionPlan {
        &self.plan
    }

    /// Clones bounded process control; this does not transfer lifecycle ownership.
    #[must_use]
    pub fn control(&self) -> ProcessControl {
        self.control.clone()
    }

    /// Waits through C4 settlement and returns the exact native terminal observation.
    ///
    /// # Errors
    /// Returns publication/observation failures; never fabricates a joined result.
    pub fn wait(self) -> Result<TerminalResult, ProductRunnerError> {
        loop {
            let observation = self.runtime.observe_preview(&self.launch)?;
            if observation.state() != PreviewProcessState::Running {
                return self
                    .control
                    .terminal_result()
                    .ok_or_else(|| failure("native terminal result unavailable"));
            }
            std::thread::sleep(super::POLL_INTERVAL);
        }
    }
}

fn failure(detail: &str) -> ProductRunnerError {
    ProductRunnerError::new(ProductRunnerErrorKind::Apply, "observe preview terminal", detail)
}
