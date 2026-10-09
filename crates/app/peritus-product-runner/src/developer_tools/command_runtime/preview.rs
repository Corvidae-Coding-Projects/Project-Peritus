//! Typed daemon-facing facade over the existing C4/C2 owned command lifecycle.

#[cfg(test)]
use std::time::Duration;

use peritus_process::OutputStream;
use serde_json::Value;

use super::{CommandRuntime, StartCommand};
mod output;
use crate::{
    PreviewCommand, PreviewLaunch, PreviewObservation, PreviewProcessState, ProductRunnerError,
    ProductRunnerErrorKind,
};
#[cfg(test)]
use output::scan_stream;

impl CommandRuntime {
    /// Starts one daemon-owned preview through the same C4/C2 authority, sandbox and process path
    /// used by ordinary developer commands.
    ///
    /// # Errors
    /// Returns a typed product error when command admission or launch fails.
    pub fn launch_preview(
        &self,
        command: &PreviewCommand,
    ) -> Result<PreviewLaunch, ProductRunnerError> {
        let started = self
            .start_owned(start_command(command))
            .map_err(|error| preview_error(error.to_string()))?;
        Ok(PreviewLaunch { handle: started.handle, process_id: started.process_id })
    }

    /// Persists the exact owner binding through the caller before any native launch effect.
    ///
    /// # Errors
    /// Rejects admission or registration failures without launching an unregistered process.
    pub fn launch_preview_registered(
        &self,
        command: &PreviewCommand,
        register: &mut dyn FnMut(crate::PreviewOwner) -> Result<(), String>,
    ) -> Result<PreviewLaunch, ProductRunnerError> {
        let mut registration = |owner: super::NativeCommandOwner| {
            register(crate::PreviewOwner::new(
                owner.source_run,
                owner.execution_run,
                owner.action,
                owner.process,
            ))
            .map_err(super::tool)
        };
        let mut request = start_command(command);
        request.owner_registered = Some(&mut registration);
        let started =
            self.start_owned(request).map_err(|error| preview_error(error.to_string()))?;
        Ok(PreviewLaunch { handle: started.handle, process_id: started.process_id })
    }

    /// Reconnects a saved binding without replaying its launch or claiming missing terminal evidence.
    ///
    /// # Errors
    /// Rejects mismatched runtime identity, conflicting ownership, or unreadable native evidence.
    pub fn reconnect_preview(
        &self,
        owner: crate::PreviewOwner,
    ) -> Result<PreviewLaunch, ProductRunnerError> {
        let native = super::NativeCommandOwner {
            source_run: owner.source_run(),
            execution_run: owner.execution_run(),
            action: owner.action(),
            process: owner.process(),
        };
        self.attach_native_owner(native).map_err(|error| preview_error(error.to_string()))?;
        Ok(PreviewLaunch {
            handle: super::identity::action_hex(owner.action()),
            process_id: owner.process(),
        })
    }

    /// Observes exact native completion independently from optional artifact publication.
    ///
    /// Output is read separately from the durable spool. A publication failure is retained as
    /// progress information and cannot erase verified process exit, cleanup, or spool facts.
    ///
    /// # Errors
    /// Rejects mismatched owners or unreadable native ownership evidence.
    pub fn observe_retained_preview(
        &self,
        owner: crate::PreviewOwner,
    ) -> Result<PreviewObservation, ProductRunnerError> {
        use peritus_process::{
            OsExitObservation, RecoveryDisposition, TerminalDisposition, TerminalRecovery,
        };
        if owner.source_run() != self.inner.run_id {
            return Err(preview_error("preview belongs to another runtime"));
        }
        let handle = super::identity::action_hex(owner.action());
        let active_owner = {
            let state = self
                .inner
                .state
                .lock()
                .map_err(|_| preview_error("command runtime is poisoned"))?;
            state.active.get(&handle).map(|active| active.plan.identity()).map(|identity| {
                identity.run_id() == owner.execution_run()
                    && identity.action_id() == owner.action()
                    && identity.process_id() == owner.process()
            })
        };
        if active_owner == Some(false) {
            return Err(preview_error("preview differs from the active native owner"));
        }
        // Polling joins the original C4 owner when it finishes. Its artifact publication policy
        // is independent of this spool-backed observer, so preserve that diagnostic separately.
        let mut probe = peritus_process::NativeProcessProbe::new();
        let mut exact = self
            .inner
            .process_store
            .observe_exact(owner.execution_run(), owner.action(), owner.process(), &mut probe)
            .map_err(|error| preview_error(error.to_string()))?;
        let mut observation = PreviewObservation {
            state: PreviewProcessState::Indeterminate,
            stdout: String::new(),
            stderr: String::new(),
            exit_code: None,
            progress: Vec::new(),
        };
        let mut draining = false;
        if active_owner == Some(true) {
            match self.poll(&handle) {
                Ok(value) => {
                    draining = value.get("state").and_then(Value::as_str) == Some("running");
                }
                Err(error) => observation
                    .progress
                    .push(format!("Command projection requires recovery: {error}")),
            }
            exact = self
                .inner
                .process_store
                .observe_exact(owner.execution_run(), owner.action(), owner.process(), &mut probe)
                .map_err(|error| preview_error(error.to_string()))?;
        }
        match exact.disposition() {
            RecoveryDisposition::LiveOwned => observation.state = PreviewProcessState::Running,
            RecoveryDisposition::Terminal => {
                let terminal = self
                    .inner
                    .process_store
                    .terminal_result(owner.process())
                    .map_err(|error| preview_error(error.to_string()))?;
                if let OsExitObservation::Code(code) = terminal.os_exit() {
                    observation.exit_code = Some(i64::from(*code));
                }
                if terminal.tree_cleanup_complete()
                    && terminal.support_tasks_joined()
                    && terminal.recovery() != TerminalRecovery::Indeterminate
                {
                    observation.state = match terminal.disposition() {
                        TerminalDisposition::Exited if observation.exit_code == Some(0) => {
                            PreviewProcessState::Succeeded
                        }
                        TerminalDisposition::Cancelled => PreviewProcessState::Cancelled,
                        TerminalDisposition::TimedOut => PreviewProcessState::TimedOut,
                        TerminalDisposition::RecoveryIndeterminate => {
                            PreviewProcessState::Indeterminate
                        }
                        _ => PreviewProcessState::Failed,
                    };
                }
                if !terminal.artifact_publication_complete() {
                    observation.progress.push("Artifact publication incomplete; exact output remains in the retained process spool".into());
                }
            }
            RecoveryDisposition::AbsentUnobserved | RecoveryDisposition::Indeterminate => {
                if draining {
                    observation.state = PreviewProcessState::Running;
                }
            }
        }
        Ok(observation)
    }

    /// Reads exact retained spool bytes even when the observing application has restarted.
    ///
    /// # Errors
    /// Rejects changed bindings, missing spools, or a range outside the observed stream.
    pub fn retained_preview_range(
        &self,
        owner: crate::PreviewOwner,
        stream: OutputStream,
        offset: u64,
        maximum_bytes: usize,
    ) -> Result<crate::PreviewOutputRange, ProductRunnerError> {
        if owner.source_run() != self.inner.run_id {
            return Err(preview_error("preview belongs to another runtime"));
        }
        let (total_bytes, bytes) = self
            .inner
            .process_store
            .spooled_stream_range_exact(
                owner.execution_run(),
                owner.action(),
                owner.process(),
                stream,
                offset,
                maximum_bytes,
            )
            .map_err(|error| preview_error(error.to_string()))?;
        Ok(crate::PreviewOutputRange { total_bytes, digest: None, bytes })
    }

    /// Polls the exact preview without transferring ownership or starting another process.
    ///
    /// # Errors
    /// Rejects unknown or corrupt runtime state.
    pub fn observe_preview(
        &self,
        launch: &PreviewLaunch,
    ) -> Result<PreviewObservation, ProductRunnerError> {
        let mut observation = self
            .poll(&launch.handle)
            .map_err(|error| preview_error(error.to_string()))
            .and_then(|value| parse_observation(&value))?;
        if observation.state == PreviewProcessState::Running {
            let control = self
                .inner
                .state
                .lock()
                .map_err(|_| preview_error("command runtime is poisoned"))?
                .active
                .get(&launch.handle)
                .and_then(|active| active.control.clone());
            if let Some(control) = control {
                let terminal = control.retained_stream_output(OutputStream::Terminal);
                let stdout = if terminal.is_empty() {
                    control.retained_stream_output(OutputStream::Stdout)
                } else {
                    terminal
                };
                observation.stdout = String::from_utf8_lossy(&stdout).into_owned();
                observation.stderr =
                    String::from_utf8_lossy(&control.retained_stream_output(OutputStream::Stderr))
                        .into_owned();
            }
        }
        Ok(observation)
    }

    /// Sends bounded bytes only to the exact interactive preview process.
    ///
    /// # Errors
    /// Rejects non-interactive, unknown, terminal or backpressured processes.
    pub fn interact_preview(
        &self,
        launch: &PreviewLaunch,
        bytes: Vec<u8>,
    ) -> Result<PreviewObservation, ProductRunnerError> {
        self.stdin(&launch.handle, bytes).map_err(|error| preview_error(error.to_string()))?;
        self.observe_preview(launch)
    }

    /// Explicitly cancels the exact owned preview and returns its latest observation.
    ///
    /// # Errors
    /// Rejects unknown state or an ambiguous control failure.
    pub fn stop_preview(
        &self,
        launch: &PreviewLaunch,
    ) -> Result<PreviewObservation, ProductRunnerError> {
        self.cancel(&launch.handle)
            .map_err(|error| preview_error(error.to_string()))
            .and_then(|value| parse_observation(&value))
    }

    /// Runs a bounded non-interactive helper through the same owned process path.
    ///
    /// This is used for capability-checked capture helpers, never for discovery.
    ///
    /// # Errors
    /// Returns a typed product error for admission, execution or projection failure.
    pub fn run_preview_helper(
        &self,
        command: &PreviewCommand,
    ) -> Result<PreviewObservation, ProductRunnerError> {
        self.run(start_command(command))
            .map_err(|error| preview_error(error.to_string()))
            .and_then(|value| parse_observation(&value))
    }
}

fn start_command(command: &PreviewCommand) -> StartCommand<'_> {
    StartCommand {
        program: &command.program,
        arguments: &command.arguments,
        cwd: &command.cwd,
        timeout: command.timeout,
        interactive: command.interactive,
        rows: command.rows,
        columns: command.columns,
        idempotency_key: &command.idempotency_key,
        environment: command.environment.clone(),
        owner_registered: None,
    }
}

fn parse_observation(value: &Value) -> Result<PreviewObservation, ProductRunnerError> {
    let state = match value.get("state").and_then(Value::as_str) {
        Some("running") => PreviewProcessState::Running,
        Some("completed") => match value.get("status").and_then(Value::as_str) {
            Some("succeeded") => PreviewProcessState::Succeeded,
            Some("cancelled") => PreviewProcessState::Cancelled,
            Some("timed_out") => PreviewProcessState::TimedOut,
            Some("indeterminate") => PreviewProcessState::Indeterminate,
            Some("failed") => PreviewProcessState::Failed,
            _ => return Err(preview_error("preview terminal status is missing or unknown")),
        },
        Some("indeterminate") => PreviewProcessState::Indeterminate,
        _ => return Err(preview_error("preview observation state is missing or unknown")),
    };
    let progress = value
        .get("progress")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| row.get("message").and_then(Value::as_str).map(str::to_owned))
        .collect();
    Ok(PreviewObservation {
        state,
        stdout: value.get("stdout").and_then(Value::as_str).unwrap_or_default().to_owned(),
        stderr: value.get("stderr").and_then(Value::as_str).unwrap_or_default().to_owned(),
        exit_code: value.get("exit_code").and_then(Value::as_i64),
        progress,
    })
}

fn preview_error(detail: impl Into<String>) -> ProductRunnerError {
    ProductRunnerError::new(ProductRunnerErrorKind::Apply, "manage preview process", detail)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "preview_backlog.rs"]
mod backlog;
