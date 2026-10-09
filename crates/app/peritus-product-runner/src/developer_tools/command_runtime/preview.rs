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
mod tests {
    use std::{
        io::{BufRead as _, Write as _},
        thread,
        time::Instant,
    };

    use peritus_types::RunId;
    use sha2::{Digest as _, Sha256};

    use super::output::{OUTPUT_SEARCH_CHUNK_BYTES, StreamScan};
    use super::*;
    use crate::{PreviewOutputMatch, PreviewOutputMatchSource, PreviewOutputStream};

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
    #[ignore = "subprocess fixture; invoked by the owned-pipe-preview test"]
    fn controlled_pipe_preview_fixture() {
        println!("READY state=0");
        std::io::stdout().flush().expect("flush readiness");
        loop {
            thread::park();
        }
    }

    #[test]
    fn owned_preview_launches_accepts_input_and_stops_explicitly() {
        exercise_owned_preview(true);
    }

    #[test]
    fn owned_pipe_preview_searches_only_its_available_live_streams() {
        exercise_owned_preview(false);
    }

    #[test]
    fn streaming_match_finds_early_midstream_and_across_chunk_boundary() {
        for (offset, expected) in [(3_usize, 3_u64), (70_003, 70_003), (65_534, 65_534)] {
            let mut data = vec![b'x'; 140_000];
            data[offset..offset + 6].copy_from_slice(b"needle");
            let mut reads = 0;
            let found = scan_stream(
                b"needle",
                u64::try_from(data.len()).expect("stream length"),
                |start, count| {
                    reads += 1;
                    assert!(count <= OUTPUT_SEARCH_CHUNK_BYTES);
                    let start = usize::try_from(start).expect("offset");
                    Ok(data[start..start + count].to_vec())
                },
            )
            .expect("search");
            assert_eq!(found.match_start, Some(expected));
            assert_eq!(reads, 3, "source digest covers the full observed prefix");
            let expected_digest: [u8; 32] = Sha256::digest(&data).into();
            assert_eq!(found.digest, expected_digest);
        }
    }

    #[test]
    fn streaming_large_miss_reads_bounded_chunks_without_lossy_utf8_aliases() {
        let data = vec![b'x'; 8 * 1024 * 1024];
        let mut reads = 0;
        let found = scan_stream(
            b"absent",
            u64::try_from(data.len()).expect("stream length"),
            |offset, count| {
                reads += 1;
                assert!(count <= OUTPUT_SEARCH_CHUNK_BYTES);
                let start = usize::try_from(offset).expect("offset");
                Ok(data[start..start + count].to_vec())
            },
        )
        .expect("large miss");
        assert_eq!(found.match_start, None);
        let expected_digest: [u8; 32] = Sha256::digest(&data).into();
        assert_eq!(found.digest, expected_digest);
        assert_eq!(reads, data.len().div_ceil(OUTPUT_SEARCH_CHUNK_BYTES));

        let invalid_utf8 = [0xff, 0xfe];
        assert_eq!(
            scan_stream(
                "�".as_bytes(),
                u64::try_from(invalid_utf8.len()).expect("stream length"),
                |offset, count| {
                    let start = usize::try_from(offset).expect("offset");
                    Ok(invalid_utf8[start..start + count].to_vec())
                },
            )
            .expect("raw-byte search"),
            StreamScan { match_start: None, digest: Sha256::digest(invalid_utf8).into() },
            "invalid source bytes must not match the replacement character created by lossy decoding"
        );
    }

    #[test]
    fn output_match_evidence_round_trips_and_rejects_invalid_ranges() {
        let evidence = PreviewOutputMatch {
            stream: PreviewOutputStream::Stdout,
            start_byte: 12,
            end_byte: 17,
            observed_stream_bytes: 20,
            matched_bytes_digest: Sha256::digest(b"ready").into(),
            source: PreviewOutputMatchSource::LiveSpool {
                process_id: [91; 16],
                observed_prefix_digest: Sha256::digest(b"0123456789abcdefghij").into(),
            },
        };
        evidence.validate().expect("valid evidence");
        evidence
            .validate_for_needle(
                peritus_types::ProcessId::new([91; 16]).expect("process id"),
                "ready",
            )
            .expect("process and needle binding");
        let encoded = serde_json::to_vec(&evidence).expect("encode");
        let decoded: PreviewOutputMatch = serde_json::from_slice(&encoded).expect("decode");
        assert_eq!(decoded, evidence);

        let mut malformed = evidence;
        malformed.end_byte = 21;
        assert!(malformed.validate().is_err());
        let mut encoded_value = serde_json::to_value(evidence).expect("encode value");
        encoded_value["source"]["live_spool"]["process_id"] =
            serde_json::json!([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        let decoded: PreviewOutputMatch = serde_json::from_value(encoded_value).expect("decode");
        assert!(decoded.validate().is_err());
    }

    #[allow(
        clippy::too_many_lines,
        reason = "one subprocess regression keeps live, terminal, restart, and cleanup assertions together"
    )]
    fn exercise_owned_preview(interactive: bool) {
        let workspace = tempfile::tempdir().expect("workspace");
        let runtime_state = tempfile::tempdir().expect("runtime state");
        let run_id = RunId::new([91; 16]).expect("run");
        let processes = peritus_process::ProcessStore::open(
            runtime_state.path().join("processes"),
            workspace.path(),
        )
        .expect("process store");
        let runtime = CommandRuntime::open(
            runtime_state.path().join("router"),
            workspace.path(),
            run_id,
            processes,
        )
        .expect("runtime");
        let executable = std::env::current_exe().expect("current test executable");
        let command = PreviewCommand::new(
            executable.to_string_lossy().into_owned(),
            vec![
                "--ignored".to_owned(),
                "--exact".to_owned(),
                if interactive {
                    "developer_tools::command_runtime::preview::tests::controlled_preview_fixture"
                } else {
                    "developer_tools::command_runtime::preview::tests::controlled_pipe_preview_fixture"
                }.to_owned(),
                "--nocapture".to_owned(),
            ],
            workspace.path().to_path_buf(),
            Duration::from_secs(10),
            interactive,
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
        let stream = if interactive { OutputStream::Terminal } else { OutputStream::Stdout };
        let live_range = runtime
            .preview_output_range(launch.process_id(), stream, readiness_offset, 9)
            .expect("live range");
        assert_eq!(live_range.bytes(), b"READY sta");
        assert!(live_range.total_bytes() >= 9);
        assert_eq!(live_range.digest(), None);
        assert!(
            runtime
                .preview_output_contains(launch.process_id(), "READY state=0")
                .expect("live match")
        );
        let exact_match = runtime
            .preview_output_match(launch.process_id(), "READY state=0")
            .expect("live output evidence")
            .expect("live output match");
        assert_eq!(exact_match.stream().process_stream(), stream);
        assert_eq!(exact_match.start_byte(), readiness_offset);
        assert_eq!(exact_match.end_byte(), readiness_offset + 13);
        let expected_match_digest: [u8; 32] = Sha256::digest(b"READY state=0").into();
        assert_eq!(exact_match.matched_bytes_digest(), expected_match_digest);
        exact_match
            .validate_for_needle(launch.process_id(), "READY state=0")
            .expect("match is bound to the launched process and literal text");
        assert!(matches!(exact_match.source(), PreviewOutputMatchSource::LiveSpool { .. }));
        assert!(
            !runtime
                .preview_output_contains(launch.process_id(), "ABSENT preview marker")
                .expect("live miss")
        );
        if interactive {
            let input = if cfg!(windows) { b"MOVE_RIGHT\r\n".as_slice() } else { b"MOVE_RIGHT\n" };
            let observation =
                runtime.interact_preview(&launch, input.to_vec()).expect("interaction");
            assert_eq!(observation.state(), PreviewProcessState::Running);
            // Observe the child's response even when interact_preview consumed the acknowledgement.
            wait_for_output(&runtime, &launch, "OBSERVED MOVE_RIGHT state=1");
        }
        let terminal = runtime.stop_preview(&launch).expect("explicit stop");
        let terminal = wait_for_terminal(&runtime, &launch, terminal);
        assert_eq!(terminal.state(), PreviewProcessState::Cancelled);
        assert!(terminal.stdout().contains("READY state=0"));
        if interactive {
            assert!(terminal.stdout().contains("OBSERVED MOVE_RIGHT state=1"));
        }
        let retained_range = runtime
            .preview_output_range(launch.process_id(), stream, readiness_offset, 9)
            .expect("finalized range");
        assert_eq!(retained_range.bytes(), b"READY sta");
        assert_eq!(retained_range.digest().map(|value| value.len()), Some(32));
        let finalized_match = runtime
            .preview_output_match(launch.process_id(), "READY state=0")
            .expect("finalized output evidence")
            .expect("finalized output match");
        assert_eq!(finalized_match.start_byte(), readiness_offset);
        assert_eq!(finalized_match.end_byte(), readiness_offset + 13);
        assert!(matches!(
            finalized_match.source(),
            PreviewOutputMatchSource::FinalizedArtifact { .. }
        ));
        assert_eq!(
            finalized_match.artifact_digest(),
            Some(finalized_match.observed_source_digest())
        );
        drop(runtime);

        let recovered_processes = peritus_process::ProcessStore::open(
            runtime_state.path().join("processes"),
            workspace.path(),
        )
        .expect("reopen process store");
        let recovered = CommandRuntime::open(
            runtime_state.path().join("router"),
            workspace.path(),
            run_id,
            recovered_processes,
        )
        .expect("reopen runtime");
        let recovered_match = recovered
            .preview_output_match(launch.process_id(), "READY state=0")
            .expect("search recovered artifact")
            .expect("recovered match");
        assert_eq!(recovered_match, finalized_match);
        recovered
            .verify_preview_output_match(exact_match, "READY state=0")
            .expect("live prefix evidence survives restart through the finalized artifact");
        recovered
            .verify_preview_output_match(recovered_match, "READY state=0")
            .expect("finalized artifact evidence survives restart");
        let mut forged_live_match = exact_match;
        forged_live_match.source = PreviewOutputMatchSource::LiveSpool {
            process_id: *launch.process_id().as_bytes(),
            observed_prefix_digest: [0; 32],
        };
        assert!(recovered.verify_preview_output_match(forged_live_match, "READY state=0").is_err());
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
