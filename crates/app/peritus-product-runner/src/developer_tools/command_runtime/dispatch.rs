use std::time::Instant;

use peritus_agent::DeveloperLoopError;
use peritus_tool_router::DispatchOutcome;

use super::{
    ActiveCommand, CommandRuntime, NativeCommandOwner, RuntimeState, TerminalCommand, result,
    retain_projection_with_owner, tool,
};

pub(super) struct StartedCommandOutcome {
    pub(super) outcome: DispatchOutcome,
    pub(super) plan: peritus_process::ExecutionPlan,
    pub(super) control: Option<peritus_process::ProcessControl>,
    pub(super) interactive: bool,
    pub(super) owner: NativeCommandOwner,
}

impl CommandRuntime {
    pub(super) fn record_started_outcome(
        &self,
        state: &mut RuntimeState,
        handle: &str,
        started: StartedCommandOutcome,
    ) -> Result<(), DeveloperLoopError> {
        match started.outcome {
            DispatchOutcome::Active(invocation) => {
                state.active.insert(
                    handle.to_owned(),
                    ActiveCommand {
                        plan: started.plan,
                        control: started.control,
                        invocation,
                        started: Instant::now(),
                        interactive: started.interactive,
                    },
                );
                retain_projection_with_owner(
                    &self.inner.state_root,
                    handle,
                    result::active(handle, &[]),
                    started.owner,
                );
            }
            DispatchOutcome::Completed(result) | DispatchOutcome::Replayed(result) => {
                let projection =
                    result::terminal(handle, &result, &self.inner.artifacts, &[]).map_err(tool)?;
                state
                    .terminal
                    .insert(handle.to_owned(), TerminalCommand { result, progress: Vec::new() });
                retain_projection_with_owner(
                    &self.inner.state_root,
                    handle,
                    projection,
                    started.owner,
                );
            }
            DispatchOutcome::PriorOutcome(disposition) => {
                return Err(tool(format!(
                    "command has prior non-replayable outcome: {disposition:?}"
                )));
            }
        }
        Ok(())
    }
}
