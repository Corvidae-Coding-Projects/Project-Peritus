//! The inner command helper keeps Git descendants separate from the receipt-writing owner.

use super::{
    OwnerBinding, POLL_INTERVAL, Request, StoreBinding, platform, read_json, save_json,
    validate_repository, validate_request,
};
use crate::{
    error::{Result, problem},
    state::save,
};
use serde::{Deserialize, Serialize};
use std::{path::Path, process::Stdio, thread};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CommandResult {
    pub(super) started: bool,
    pub(super) cancelled: bool,
    pub(super) status: Option<i32>,
    pub(super) error: Option<String>,
}

pub(super) fn run(request_path: &Path) -> Result<()> {
    let request_path = request_path.canonicalize()?;
    if request_path.file_name().and_then(|name| name.to_str()) != Some("request.json") {
        return Err(problem("The Git command helper received an invalid request path"));
    }
    let directory = request_path
        .parent()
        .ok_or_else(|| problem("The Git command request has no owner directory"))?;
    let request: Request = read_json(&request_path)?;
    let store = StoreBinding::from_parts(&request.state_file, request.workspace.clone())?;
    if directory != store.directory(&request.operation) {
        return Err(problem("The Git command request is outside its owning store"));
    }
    validate_request(&store, &request, None)?;
    validate_repository(&request.command.repository)?;

    let job_name = job_name(&request);
    let binding = platform::command_binding(&job_name)?;
    save_json(&directory.join("command.json"), &binding)?;
    match await_activation(directory, &request)? {
        Activation::Cancelled => {
            save_result(directory, false, true, None, None)?;
            return Ok(());
        }
        Activation::Activated => {}
    }

    let containment = platform::activate_command(&binding, &job_name)?;
    save(
        &directory.join("command-contained"),
        request.descriptor.as_bytes(),
    )?;
    if directory.join("cancel").is_file() {
        save_result(directory, false, true, None, None)?;
        platform::end_command(containment);
    }

    save(&directory.join("command-starting"), request.descriptor.as_bytes())?;
    let mut command = std::process::Command::new("git");
    command
        .current_dir(&request.command.repository)
        .arg("--no-pager")
        .args(&request.command.args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null());
    platform::configure_git_child(&mut command);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            save_result(directory, false, false, None, Some(error.to_string()))?;
            platform::end_command(containment);
        }
    };
    save(&directory.join("command-started"), request.descriptor.as_bytes())?;
    let (status, error) = match child.wait() {
        Ok(status) => (status.code().or_else(|| status.success().then_some(0)), None),
        Err(error) => (None, Some(format!("Git process wait failed: {error}"))),
    };
    save_result(directory, true, false, status, error)?;
    platform::end_command(containment)
}

pub(super) fn job_name(request: &Request) -> String {
    format!("{}-Command", request.job_name)
}

pub(super) fn read_binding(directory: &Path) -> Result<Option<OwnerBinding>> {
    super::read_optional(&directory.join("command.json"))
}

pub(super) fn read_result(directory: &Path) -> Result<Option<CommandResult>> {
    super::read_optional(&directory.join("command-result.json"))
}

fn save_result(
    directory: &Path,
    started: bool,
    cancelled: bool,
    status: Option<i32>,
    error: Option<String>,
) -> Result<()> {
    save_json(
        &directory.join("command-result.json"),
        &CommandResult { started, cancelled, status, error },
    )
}

enum Activation {
    Activated,
    Cancelled,
}

fn await_activation(directory: &Path, request: &Request) -> Result<Activation> {
    loop {
        if directory.join("cancel").is_file() {
            return Ok(Activation::Cancelled);
        }
        match std::fs::read_to_string(directory.join("command-activate")) {
            Ok(value) if value == request.descriptor => return Ok(Activation::Activated),
            Ok(_) => return Err(problem("The Git command activation does not match its receipt")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                thread::sleep(POLL_INTERVAL);
            }
            Err(error) => return Err(error.into()),
        }
    }
}
