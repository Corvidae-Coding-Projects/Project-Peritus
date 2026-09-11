//! Typed daemon-facing facade over the existing C4/C2 owned command lifecycle.

#[cfg(test)]
use std::time::Duration;

use serde_json::Value;

use super::{CommandRuntime, StartCommand};
use crate::{
    PreviewCommand, PreviewLaunch, PreviewObservation, PreviewProcessState, ProductRunnerError,
    ProductRunnerErrorKind,
};

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

    /// Polls the exact preview without transferring ownership or starting another process.
    ///
    /// # Errors
    /// Rejects unknown or corrupt runtime state.
    pub fn observe_preview(
        &self,
        launch: &PreviewLaunch,
    ) -> Result<PreviewObservation, ProductRunnerError> {
        self.poll(&launch.handle)
            .map_err(|error| preview_error(error.to_string()))
            .and_then(|value| parse_observation(&value))
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
        self.stdin(&launch.handle, bytes)
            .map_err(|error| preview_error(error.to_string()))
            .and_then(|value| parse_observation(&value))
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
mod tests {
    use std::{
        io::{BufRead as _, Write as _},
        thread,
        time::Instant,
    };

    use peritus_types::RunId;

    use super::*;

    #[test]
    #[ignore = "subprocess fixture; invoked by the owned-preview test"]
    fn controlled_preview_fixture() {
        println!("READY state=0");
        std::io::stdout().flush().expect("flush readiness");
        let mut lines = std::io::BufReader::new(std::io::stdin()).lines();
        let movement = lines.next().expect("movement input").expect("movement bytes");
        println!("OBSERVED {movement} state=1");
        std::io::stdout().flush().expect("flush observed state");
        let _ = lines.next();
    }

    #[test]
    fn owned_preview_launches_accepts_input_and_stops_explicitly() {
        let workspace = tempfile::tempdir().expect("workspace");
        let runtime =
            CommandRuntime::open_for_test(workspace.path(), RunId::new([91; 16]).expect("run"));
        let executable = std::env::current_exe().expect("current test executable");
        let command = PreviewCommand::new(
            executable.to_string_lossy().into_owned(),
            vec![
                "--ignored".to_owned(),
                "--exact".to_owned(),
                "developer_tools::command_runtime::preview::tests::controlled_preview_fixture"
                    .to_owned(),
                "--nocapture".to_owned(),
            ],
            workspace.path().to_path_buf(),
            Duration::from_secs(10),
            true,
            24,
            80,
            "owned-preview-normal-flow".to_owned(),
            Vec::new(),
        )
        .expect("profile");
        let launch = runtime.launch_preview(&command).expect("launch");
        assert_ne!(launch.process_id().as_bytes(), &[0; 16]);
        wait_for_progress(&runtime, &launch, "started");
        let observation =
            runtime.interact_preview(&launch, b"MOVE_RIGHT\n".to_vec()).expect("interaction");
        assert_eq!(observation.state(), PreviewProcessState::Running);
        wait_for_progress(&runtime, &launch, "stdin-accepted");
        let terminal = runtime.stop_preview(&launch).expect("explicit stop");
        let terminal = wait_for_terminal(&runtime, &launch, terminal);
        assert_eq!(terminal.state(), PreviewProcessState::Cancelled);
        assert!(terminal.stdout().contains("READY state=0"));
        assert!(terminal.stdout().contains("OBSERVED MOVE_RIGHT state=1"));
    }

    fn wait_for_progress(runtime: &CommandRuntime, launch: &PreviewLaunch, expected: &str) {
        let began = Instant::now();
        loop {
            let observed = runtime.observe_preview(launch).expect("observe");
            if observed.progress().iter().any(|message| message.contains(expected)) {
                return;
            }
            assert!(began.elapsed() < Duration::from_secs(5), "missing {expected}");
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn wait_for_terminal(
        runtime: &CommandRuntime,
        launch: &PreviewLaunch,
        mut observed: PreviewObservation,
    ) -> PreviewObservation {
        let began = Instant::now();
        while observed.state() == PreviewProcessState::Running {
            assert!(began.elapsed() < Duration::from_secs(5), "preview did not stop");
            thread::sleep(Duration::from_millis(10));
            observed = runtime.observe_preview(launch).expect("observe terminal");
        }
        observed
    }
}
