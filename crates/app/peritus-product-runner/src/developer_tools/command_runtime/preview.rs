//! Typed daemon-facing facade over the existing C4/C2 owned command lifecycle.

#[cfg(test)]
use std::time::Duration;

use peritus_artifact_store::{ArtifactDigest, ArtifactStore};
use peritus_process::OutputStream;
use serde_json::Value;

use super::{CommandRuntime, StartCommand};
use crate::{
    PreviewCommand, PreviewLaunch, PreviewObservation, PreviewOutputRange, PreviewProcessState,
    ProductRunnerError, ProductRunnerErrorKind,
};

impl CommandRuntime {
    /// Reads one bounded range from an exact live spool or finalized output artifact.
    ///
    /// # Errors
    /// Returns an error when the process or its durable stream cannot be verified.
    pub fn preview_output_range(
        &self,
        process_id: peritus_types::ProcessId,
        stream: OutputStream,
        offset: u64,
        maximum_bytes: usize,
    ) -> Result<PreviewOutputRange, ProductRunnerError> {
        if maximum_bytes == 0 {
            return Err(preview_error("preview output range size must be positive"));
        }
        let active = self
            .inner
            .state
            .lock()
            .map_err(|_| preview_error("command runtime is poisoned"))?
            .active
            .values()
            .find(|command| command.plan.identity().process_id() == process_id)
            .and_then(|command| command.control.clone());
        if let Some(control) = active {
            let (total_bytes, bytes) = control
                .spooled_stream_range(stream, offset, maximum_bytes)
                .map_err(|error| preview_error(error.to_string()))?;
            return Ok(PreviewOutputRange { total_bytes, digest: None, bytes });
        }
        let terminal = self
            .inner
            .process_store
            .terminal_result(process_id)
            .map_err(|error| preview_error(error.to_string()))?;
        let artifact = terminal
            .artifacts()
            .iter()
            .find(|artifact| artifact.stream() == stream)
            .ok_or_else(|| preview_error("finalized output stream artifact is unavailable"))?;
        if offset > artifact.size() {
            return Err(preview_error("preview output range begins past the finalized stream"));
        }
        let store = ArtifactStore::open(self.inner.artifacts.clone())
            .map_err(|error| preview_error(format!("reopen output artifact store: {error}")))?;
        let mut reader = store
            .open_read(ArtifactDigest::from_sha256(artifact.digest()))
            .map_err(|error| preview_error(format!("open output artifact: {error}")))?;
        let mut bytes = Vec::new();
        while let Some(chunk) = reader
            .read_chunk(64 * 1024)
            .map_err(|error| preview_error(format!("read output artifact: {error}")))?
        {
            let end = chunk
                .offset()
                .saturating_add(u64::try_from(chunk.bytes().len()).unwrap_or(u64::MAX));
            if end > offset {
                let skip =
                    usize::try_from(offset.saturating_sub(chunk.offset())).unwrap_or(usize::MAX);
                let available = &chunk.bytes()[skip.min(chunk.bytes().len())..];
                let remaining = maximum_bytes.saturating_sub(bytes.len());
                bytes.extend_from_slice(&available[..available.len().min(remaining)]);
                if bytes.len() == maximum_bytes {
                    break;
                }
            }
            if end >= artifact.size() {
                break;
            }
        }
        Ok(PreviewOutputRange {
            total_bytes: artifact.size(),
            digest: Some(*artifact.digest().as_bytes()),
            bytes,
        })
    }

    /// Checks the exact process's complete spool or published output artifacts for a string.
    ///
    /// # Errors
    /// Returns an error when the process output evidence cannot be reopened or read.
    pub fn preview_output_contains(
        &self,
        process_id: peritus_types::ProcessId,
        needle: &str,
    ) -> Result<bool, ProductRunnerError> {
        use peritus_process::OutputStream;
        let active = self
            .inner
            .state
            .lock()
            .map_err(|_| preview_error("command runtime is poisoned"))?
            .active
            .values()
            .find(|command| command.plan.identity().process_id() == process_id)
            .and_then(|command| command.control.clone());
        if let Some(control) = active {
            for stream in [OutputStream::Stdout, OutputStream::Stderr, OutputStream::Terminal] {
                let bytes = control
                    .full_spooled_stream_output(stream)
                    .map_err(|error| preview_error(error.to_string()))?;
                if String::from_utf8_lossy(&bytes).contains(needle) {
                    return Ok(true);
                }
            }
            return Ok(false);
        }
        let terminal = self
            .inner
            .process_store
            .terminal_result(process_id)
            .map_err(|error| preview_error(error.to_string()))?;
        super::result::durable_output_contains(&terminal, &self.inner.artifacts, needle)
            .map_err(preview_error)
    }

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
                use peritus_process::OutputStream;
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
        let began = Instant::now();
        let readiness_offset = loop {
            let observation = runtime.observe_preview(&launch).expect("live output");
            assert_eq!(observation.state(), PreviewProcessState::Running);
            if let Some(offset) = observation.stdout().find("READY state=0") {
                break u64::try_from(offset).expect("readiness offset");
            }
            assert!(
                began.elapsed() < Duration::from_secs(5),
                "readiness must be visible before exit or input"
            );
            thread::sleep(Duration::from_millis(10));
        };
        let live_range = runtime
            .preview_output_range(launch.process_id(), OutputStream::Terminal, readiness_offset, 9)
            .expect("live range");
        assert_eq!(live_range.bytes(), b"READY sta");
        assert!(live_range.total_bytes() >= 9);
        assert_eq!(live_range.digest(), None);
        let observation =
            runtime.interact_preview(&launch, b"MOVE_RIGHT\n".to_vec()).expect("interaction");
        assert_eq!(observation.state(), PreviewProcessState::Running);
        // interact_preview may consume the stdin acknowledgement itself. Confirm the
        // child actually processed the bytes before stopping it, independent of poll timing.
        wait_for_output(&runtime, &launch, "OBSERVED MOVE_RIGHT state=1");
        let terminal = runtime.stop_preview(&launch).expect("explicit stop");
        let terminal = wait_for_terminal(&runtime, &launch, terminal);
        assert_eq!(terminal.state(), PreviewProcessState::Cancelled);
        assert!(terminal.stdout().contains("READY state=0"));
        assert!(terminal.stdout().contains("OBSERVED MOVE_RIGHT state=1"));
        let retained_range = runtime
            .preview_output_range(launch.process_id(), OutputStream::Terminal, readiness_offset, 9)
            .expect("finalized range");
        assert_eq!(retained_range.bytes(), b"READY sta");
        assert_eq!(retained_range.digest().map(|value| value.len()), Some(32));
    }

    fn wait_for_output(runtime: &CommandRuntime, launch: &PreviewLaunch, expected: &str) {
        let began = Instant::now();
        loop {
            let observed = runtime.observe_preview(launch).expect("observe");
            if observed.stdout().contains(expected) {
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

#[cfg(test)]
#[path = "preview_backlog.rs"]
mod backlog;
