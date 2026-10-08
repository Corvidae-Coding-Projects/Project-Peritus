//! Detached sidecar lifecycle and the sole start/no-start publication authority.

use super::{
    COMMAND_FLAG, EffectResult, EffectState, Phase, POLL_INTERVAL, Request, SCHEMA_VERSION,
    StoreBinding, WATCHDOG_FLAG, command, digest, observe, output, platform, read_json, read_state,
    save_json,
    validate_repository, validate_request,
};
use crate::{
    error::{Result, problem},
    state::save,
};
use peritus_process::ProbeObservation;
use std::{
    path::Path,
    process::{Child, Stdio},
    thread::{self, JoinHandle},
};

enum Activation {
    Activated,
    Cancelled,
}

pub(super) fn run(request_path: &Path) -> Result<()> {
    let request_path = request_path.canonicalize()?;
    if request_path.file_name().and_then(|name| name.to_str()) != Some("request.json") {
        return Err(problem("The Git effect owner received an invalid request path"));
    }
    let directory = request_path
        .parent()
        .ok_or_else(|| problem("The Git effect request has no owner directory"))?;
    let request: Request = read_json(&request_path)?;
    let store_root = directory
        .parent()
        .ok_or_else(|| problem("The Git effect request has no owning store"))?;
    let store = StoreBinding::from_parts(&request.state_file, request.workspace.clone())?;
    if store_root != store.root
        || store_root.file_name().and_then(|name| name.to_str()) != Some(request.store.as_str())
        || directory.file_name().and_then(|name| name.to_str())
            != Some(digest(request.operation.as_bytes()).as_str())
    {
        return Err(problem("The Git effect request is outside its owning store"));
    }
    validate_request(&store, &request, None)?;
    validate_repository(&request.command.repository)?;

    let (mut containment, owner) = platform::activate_owner(&request.job_name)?;
    let mut state = EffectState {
        schema_version: SCHEMA_VERSION,
        workspace: request.workspace.clone(),
        store: request.store.clone(),
        operation: request.operation.clone(),
        descriptor: request.descriptor.clone(),
        phase: Phase::Prepared,
        owner,
        result: None,
    };
    save_json(&directory.join("state.json"), &state)?;
    let watchdog = match platform::start_watchdog(directory, &state.owner, WATCHDOG_FLAG) {
        Ok(watchdog) => watchdog,
        Err(error) => {
            state.phase = Phase::Completed;
            state.result = Some(setup_result(format!(
                "The Git owner watchdog could not start: {}",
                error.0,
            )));
            save_json(&directory.join("state.json"), &state)?;
            platform::complete(&mut containment);
            return Ok(());
        }
    };

    let result = match wait_for_activation(directory, &request)? {
        Activation::Cancelled => cancelled_result(),
        Activation::Activated if directory.join("cancel").is_file() => cancelled_result(),
        Activation::Activated => run_git(directory, &request, &mut state)?,
    };
    state.phase = Phase::Completed;
    state.result = Some(result);
    save_json(&directory.join("state.json"), &state)?;
    finish(directory, &mut containment, watchdog)
}

pub(super) fn run_watchdog(directory: &Path) -> Result<()> {
    let directory = directory.canonicalize()?;
    let state = read_state(&directory)?
        .ok_or_else(|| problem("The Git watchdog owner record is missing"))?;
    platform::validate_watchdog(&state.owner)?;
    loop {
        if directory.join("owner-complete").is_file() {
            // The watchdog remains inside the dedicated group so its presence pins the group
            // identity. A terminal receipt is durable now; reap any descendant that outlived Git.
            terminate_command_boundary(&directory);
            platform::watchdog_terminate(&state.owner);
        }
        match observe(&state.owner) {
            Ok(ProbeObservation::ExactLive) => thread::sleep(POLL_INTERVAL),
            Ok(
                ProbeObservation::ExactAbsent
                | ProbeObservation::Mismatched
                | ProbeObservation::Unverifiable,
            )
            | Err(_) => {
                terminate_command_boundary(&directory);
                platform::watchdog_terminate(&state.owner)
            }
        }
    }
}

fn wait_for_activation(directory: &Path, request: &Request) -> Result<Activation> {
    loop {
        if directory.join("cancel").is_file() {
            return Ok(Activation::Cancelled);
        }
        match std::fs::read_to_string(directory.join("activate")) {
            Ok(value) if value == request.descriptor => return Ok(Activation::Activated),
            Ok(_) => return Err(problem("The Git effect activation does not match its receipt")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                thread::sleep(POLL_INTERVAL);
            }
            Err(error) => return Err(error.into()),
        }
    }
}

fn run_git(
    directory: &Path,
    request: &Request,
    state: &mut EffectState,
) -> Result<EffectResult> {
    let mut runner = std::process::Command::new(std::env::current_exe()?);
    runner
        .arg(COMMAND_FLAG)
        .arg(directory.join("request.json"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    platform::configure_command_runner(&mut runner);
    let mut child = match runner.spawn() {
        Ok(child) => child,
        Err(error) => return Ok(setup_result(error.to_string())),
    };
    let runner_pid = child.id();
    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            cleanup_command(directory, &mut child);
            return Ok(setup_result("Git stdout was not captured".into()));
        }
    };
    let stderr = match child.stderr.take() {
        Some(stderr) => stderr,
        None => {
            drop(stdout);
            cleanup_command(directory, &mut child);
            return Ok(setup_result("Git stderr was not captured".into()));
        }
    };
    let stdout_path = directory.join("stdout");
    let stdout_capture = match thread::Builder::new()
        .name("peritus-git-stdout".into())
        .spawn(move || output::capture(stdout, &stdout_path))
    {
        Ok(capture) => capture,
        Err(error) => {
            drop(stderr);
            cleanup_command(directory, &mut child);
            return Ok(setup_result(format!("Git stdout capture could not start: {error}")));
        }
    };
    let stderr_path = directory.join("stderr");
    let stderr_capture = match thread::Builder::new()
        .name("peritus-git-stderr".into())
        .spawn(move || output::capture(stderr, &stderr_path))
    {
        Ok(capture) => capture,
        Err(error) => {
            cleanup_command(directory, &mut child);
            let _ = join_capture(stdout_capture, "stdout");
            return Ok(setup_result(format!("Git stderr capture could not start: {error}")));
        }
    };

    let binding = loop {
        let observed = match command::read_binding(directory) {
            Ok(observed) => observed,
            Err(error) => {
                cleanup_command(directory, &mut child);
                let _ = join_captures(stdout_capture, stderr_capture);
                return Err(error);
            }
        };
        if let Some(binding) = observed {
            if binding.pid != runner_pid {
                cleanup_command(directory, &mut child);
                let captures = join_captures(stdout_capture, stderr_capture);
                return Ok(started_failure(
                    false,
                    "The Git command helper receipt has the wrong process identity".into(),
                    captures,
                ));
            }
            if let Err(error) =
                platform::validate_owner_binding(&binding, &command::job_name(request))
            {
                cleanup_command(directory, &mut child);
                let _ = join_captures(stdout_capture, stderr_capture);
                return Err(error);
            }
            if let Err(error) = platform::validate_owned_command(&binding, &child) {
                cleanup_command(directory, &mut child);
                let _ = join_captures(stdout_capture, stderr_capture);
                return Err(error);
            }
            break binding;
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                let captures = join_captures(stdout_capture, stderr_capture);
                return Ok(started_failure(
                    false,
                    format!("The Git command helper stopped before activation ({status})"),
                    captures,
                ));
            }
            Ok(None) => {}
            Err(error) => {
                cleanup_command(directory, &mut child);
                let _ = join_captures(stdout_capture, stderr_capture);
                return Err(error.into());
            }
        }
        thread::sleep(POLL_INTERVAL);
    };
    if directory.join("cancel").is_file() {
        cleanup_command(directory, &mut child);
        let _ = join_captures(stdout_capture, stderr_capture);
        return Ok(cancelled_result());
    }
    if let Err(error) = save(
        &directory.join("command-activate"),
        request.descriptor.as_bytes(),
    ) {
        cleanup_command(directory, &mut child);
        let _ = join_captures(stdout_capture, stderr_capture);
        return Err(error);
    }

    loop {
        match command_marker(directory, "command-started", request) {
            Ok(true) => break,
            Ok(false) => {}
            Err(error) => {
                cleanup_command(directory, &mut child);
                let _ = join_captures(stdout_capture, stderr_capture);
                return Err(error);
            }
        }
        let result = match command::read_result(directory) {
            Ok(result) => result,
            Err(error) => {
                cleanup_command(directory, &mut child);
                let _ = join_captures(stdout_capture, stderr_capture);
                return Err(error);
            }
        };
        if let Some(result) = result {
            cleanup_command(directory, &mut child);
            let captures = join_captures(stdout_capture, stderr_capture);
            return Ok(result_before_start(result, captures));
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                cleanup_command(directory, &mut child);
                let started = match command_marker(directory, "command-starting", request) {
                    Ok(started) => started,
                    Err(error) => {
                        let _ = join_captures(stdout_capture, stderr_capture);
                        return Err(error);
                    }
                };
                let captures = join_captures(stdout_capture, stderr_capture);
                return Ok(started_failure(
                    started,
                    format!("The Git command helper stopped before Git started ({status})"),
                    captures,
                ));
            }
            Ok(None) => {}
            Err(error) => {
                cleanup_command(directory, &mut child);
                let _ = join_captures(stdout_capture, stderr_capture);
                return Err(error.into());
            }
        }
        thread::sleep(POLL_INTERVAL);
    }

    state.phase = Phase::Running;
    if let Err(error) = save_json(&directory.join("state.json"), state) {
        let _ = platform::terminate_owned_command(&binding, &child);
        cleanup_command(directory, &mut child);
        let _ = join_captures(stdout_capture, stderr_capture);
        return Err(error);
    }
    let mut cancellation_requested = false;
    let mut cancellation_error = None;
    // React to either complete output EOF or direct-helper exit. The latter is observed without
    // reaping so the owned child still pins the exact Unix group identity while descendants that
    // inherited a pipe are terminated. Polling is cadence-bounded and never counts zombies.
    let exit_observation_error = loop {
        if !cancellation_requested && directory.join("cancel").is_file() {
            cancellation_requested = true;
            cancellation_error = platform::terminate_owned_command(&binding, &child)
                .err()
                .map(|error| error.0);
        }
        if stdout_capture.is_finished() && stderr_capture.is_finished() {
            break None;
        }
        match platform::owned_command_exited(&binding, &child) {
            Ok(true) => break None,
            Ok(false) => thread::sleep(POLL_INTERVAL),
            Err(error) => break Some(error.0),
        }
    };
    let boundary_error = platform::terminate_owned_command(&binding, &child)
        .err()
        .map(|error| error.0);
    let (stdout, stderr) = join_captures(stdout_capture, stderr_capture);
    let drain_error = platform::drain_command(&binding, &mut child).err().map(|error| error.0);
    let result = command::read_result(directory)?;
    let status = result.as_ref().and_then(|result| result.status);
    let cancelled = result
        .as_ref()
        .map_or_else(|| directory.join("cancel").is_file(), |result| result.cancelled);
    let command_error = result
        .as_ref()
        .and_then(|result| result.error.clone())
        .or_else(|| result.as_ref().filter(|result| !result.started).map(|_| {
            "The Git command helper published an invalid no-start result after start".into()
        }))
        .or_else(|| result.is_none().then(|| {
            if cancelled {
                "The Git command boundary ended after durable cancellation before the helper published its result; repository effects may be partial"
            } else {
                "The Git command helper omitted its result"
            }
            .into()
        }));
    let capture_error = (!stdout.complete || !stderr.complete)
        .then(|| "Git output could not be retained completely".to_owned());
    Ok(EffectResult {
        started: true,
        cancelled,
        status,
        internal_error: cancellation_error
            .or(exit_observation_error)
            .or(boundary_error)
            .or(drain_error)
            .or(command_error)
            .or(capture_error),
        stdout,
        stderr,
    })
}

fn result_before_start(
    result: command::CommandResult,
    (stdout, stderr): (output::Capture, output::Capture),
) -> EffectResult {
    EffectResult {
        started: result.started,
        cancelled: result.cancelled,
        status: result.status,
        internal_error: result.error,
        stdout,
        stderr,
    }
}

fn started_failure(
    started: bool,
    message: String,
    (stdout, stderr): (output::Capture, output::Capture),
) -> EffectResult {
    EffectResult {
        started,
        cancelled: false,
        status: None,
        internal_error: Some(message),
        stdout,
        stderr,
    }
}

fn setup_result(message: String) -> EffectResult {
    EffectResult {
        started: false,
        cancelled: false,
        status: None,
        internal_error: Some(message),
        stdout: output::Capture::empty(),
        stderr: output::Capture::empty(),
    }
}

fn cancelled_result() -> EffectResult {
    EffectResult {
        started: false,
        cancelled: true,
        status: None,
        internal_error: None,
        stdout: output::Capture::empty(),
        stderr: output::Capture::empty(),
    }
}

fn cleanup_command(directory: &Path, child: &mut Child) {
    if let Ok(Some(binding)) = command::read_binding(directory) {
        let _ = platform::terminate_owned_command(&binding, child);
        let _ = platform::drain_command(&binding, child);
        return;
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn terminate_command_boundary(directory: &Path) {
    if let Ok(Some(binding)) = command::read_binding(directory) {
        let _ = platform::terminate_command(&binding);
    }
}

fn command_marker(directory: &Path, name: &str, request: &Request) -> Result<bool> {
    match std::fs::read_to_string(directory.join(name)) {
        Ok(value) if value == request.descriptor => Ok(true),
        Ok(_) => Err(problem("The Git command marker does not match its command receipt")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn join_captures(
    stdout: JoinHandle<output::Capture>,
    stderr: JoinHandle<output::Capture>,
) -> (output::Capture, output::Capture) {
    (join_capture(stdout, "stdout"), join_capture(stderr, "stderr"))
}

fn join_capture(handle: JoinHandle<output::Capture>, stream: &str) -> output::Capture {
    handle.join().unwrap_or_else(|_| {
        output::Capture::failed(format!("Git {stream} capture thread panicked"))
    })
}

fn finish(
    directory: &Path,
    containment: &mut platform::Containment,
    watchdog: platform::Watchdog,
) -> Result<()> {
    save(&directory.join("owner-complete"), b"owner-complete-v3\n")?;
    platform::finish_watchdog(watchdog)?;
    platform::complete(containment);
    Ok(())
}
