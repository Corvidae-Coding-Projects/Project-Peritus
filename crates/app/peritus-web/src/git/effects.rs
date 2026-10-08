//! Durable ownership for Git commands that may run hooks, signers, or network children.

mod command;
mod output;
mod owner;
mod platform;
mod recovery;

use crate::{
    error::{Result, problem, uncertain},
    state::{App, OperationOwner, hex, save},
};
use peritus_process::{NativeProcessProbe, ProbeObservation, ProcessProbe, ProcessTreeIdentity};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{path::{Path, PathBuf}, process::Stdio, time::Duration};

pub(super) const SCHEMA_VERSION: u16 = 3;
pub(super) const OWNER_FLAG: &str = "--peritus-git-effect-owner";
pub(super) const WATCHDOG_FLAG: &str = "--peritus-git-effect-watchdog";
pub(super) const COMMAND_FLAG: &str = "--peritus-git-effect-command";
pub(super) const POLL_INTERVAL: Duration = Duration::from_millis(40);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Descriptor {
    pub(super) repository: PathBuf,
    pub(super) args: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Request {
    pub(super) schema_version: u16,
    pub(super) workspace: String,
    pub(super) store: String,
    pub(super) state_file: PathBuf,
    pub(super) operation: String,
    pub(super) descriptor: String,
    pub(super) job_name: String,
    pub(super) command: Descriptor,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OwnerBinding {
    pub(super) pid: u32,
    pub(super) start_token: u64,
    pub(super) process_group: Option<u32>,
    pub(super) complete_containment: bool,
    pub(super) job_name: Option<String>,
}

pub(super) struct DetachedContainment(platform::Containment);

pub(super) fn configure_detached_owner(command: &mut tokio::process::Command) {
    platform::configure_owner_command(command);
}

pub(crate) fn detached_launcher_binding(pid: u32) -> Result<OwnerBinding> {
    platform::launcher_binding(pid)
}

pub(crate) fn validate_detached_launcher(binding: &OwnerBinding) -> Result<()> {
    platform::validate_launcher_binding(binding)
}

pub(super) fn activate_detached_owner(
    job_name: &str,
) -> Result<(DetachedContainment, OwnerBinding)> {
    let (containment, binding) = platform::activate_owner(job_name)?;
    Ok((DetachedContainment(containment), binding))
}

pub(super) fn validate_detached_owner(binding: &OwnerBinding, job_name: &str) -> Result<()> {
    platform::validate_owner_binding(binding, job_name)
}

pub(crate) fn observe_detached_owner(binding: &OwnerBinding) -> Result<ProbeObservation> {
    observe(binding)
}

pub(super) fn terminate_detached_owner(binding: &OwnerBinding) -> Result<()> {
    platform::terminate(binding)
}

pub(super) fn complete_detached_owner(containment: &mut DetachedContainment) {
    platform::complete(&mut containment.0);
}

impl OwnerBinding {
    pub(super) const fn identity(&self) -> ProcessTreeIdentity {
        ProcessTreeIdentity::new(
            self.pid,
            Some(self.start_token),
            self.process_group,
            self.complete_containment,
        )
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Phase {
    Prepared,
    Running,
    Completed,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EffectState {
    pub(super) schema_version: u16,
    pub(super) workspace: String,
    pub(super) store: String,
    pub(super) operation: String,
    pub(super) descriptor: String,
    pub(super) phase: Phase,
    pub(super) owner: OwnerBinding,
    pub(super) result: Option<EffectResult>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EffectResult {
    pub(super) started: bool,
    pub(super) cancelled: bool,
    pub(super) status: Option<i32>,
    pub(super) internal_error: Option<String>,
    pub(super) stdout: output::Capture,
    pub(super) stderr: output::Capture,
}

#[derive(Clone, Debug)]
pub(super) struct StoreBinding {
    pub(super) root: PathBuf,
    pub(super) workspace: String,
    pub(super) identity: String,
    pub(super) state_file: PathBuf,
}

impl StoreBinding {
    fn open(app: &App) -> Result<Self> {
        let workspace = app.snapshot()?.identity;
        Self::from_parts(&app.options.state_file, workspace)
    }

    pub(super) fn from_parts(state_file: &Path, workspace: String) -> Result<Self> {
        let state_file = state_file.canonicalize()?;
        let mut owner = Sha256::new();
        owner.update(b"peritus-web-git-effect-store-v3\0");
        owner.update(workspace.as_bytes());
        owner.update(b"\0");
        owner.update(state_file.to_string_lossy().as_bytes());
        let identity = hex(&owner.finalize());
        let root = state_file
            .parent()
            .ok_or_else(|| problem("The gateway state file has no parent directory"))?
            .join("git-effects")
            .join(&identity);
        Ok(Self { root, workspace, identity, state_file })
    }

    pub(super) fn directory(&self, operation: &str) -> PathBuf {
        self.root.join(digest(operation.as_bytes()))
    }
}

pub(crate) struct Recovery {
    pub(crate) result: Option<Value>,
    pub(crate) observation: Value,
}

pub(super) async fn execute(
    app: &App,
    operation_owner: &OperationOwner,
    repository: &Path,
    args: &[String],
) -> Result<Value> {
    let store = StoreBinding::open(app)?;
    let request = prepare(&store, operation_owner, repository, args)?;
    let directory = store.directory(operation_owner.identity());
    if let Some(state) = read_state(&directory).map_err(|error| {
        uncertain(format!("The retained Git effect state could not be read: {}", error.0))
    })? {
        validate_state(&request, &state)?;
        return recovery::await_state(&directory, &request, state).await;
    }
    let mut child = launch_owner(&directory)?;
    let launch = match child
        .id()
        .ok_or_else(|| problem("The Git effect owner has no operating-system identity"))
        .and_then(platform::launcher_binding)
    {
        Ok(launch) => launch,
        Err(error) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(error);
        }
    };
    if let Err(error) = save_json(&directory.join("launch.json"), &launch) {
        let _ = child.kill().await;
        let _ = child.wait().await;
        return Err(error);
    }
    recovery::await_activation(directory, request, child).await
}

pub(crate) fn owner_argument() -> Option<PathBuf> {
    hidden_argument(OWNER_FLAG)
}

pub(crate) fn watchdog_argument() -> Option<PathBuf> {
    hidden_argument(WATCHDOG_FLAG)
}

pub(crate) fn command_argument() -> Option<PathBuf> {
    hidden_argument(COMMAND_FLAG)
}

fn hidden_argument(flag: &str) -> Option<PathBuf> {
    let mut args = std::env::args_os();
    let _program = args.next()?;
    if args.next()?.to_str()? != flag {
        return None;
    }
    let path = PathBuf::from(args.next()?);
    args.next().is_none().then_some(path)
}

pub(crate) fn run_owner(request_path: &Path) -> Result<()> {
    owner::run(request_path)
}

pub(crate) fn run_watchdog(directory: &Path) -> Result<()> {
    owner::run_watchdog(directory)
}

pub(crate) fn run_command(request_path: &Path) -> Result<()> {
    command::run(request_path)
}

pub(crate) async fn cancel(app: &App, operation: &str, prepared: Option<&Value>) -> Result<Value> {
    let store = StoreBinding::open(app)?;
    let operation = operation.to_owned();
    let prepared = prepared.cloned();
    tokio::task::spawn_blocking(move || recovery::cancel(&store, &operation, prepared.as_ref()))
        .await
        .map_err(problem)?
}

pub(crate) async fn recover(
    app: &App,
    operation: &str,
    prepared: Option<&Value>,
) -> Result<Recovery> {
    let store = StoreBinding::open(app)?;
    let operation = operation.to_owned();
    let prepared = prepared.cloned();
    tokio::task::spawn_blocking(move || recovery::recover(&store, &operation, prepared.as_ref()))
        .await
        .map_err(problem)?
}

pub(crate) async fn output_page(
    app: &App,
    operation: &str,
    prepared: Option<&Value>,
    stream: &str,
    offset: u64,
) -> Result<Value> {
    let store = StoreBinding::open(app)?;
    let operation = operation.to_owned();
    let prepared = prepared.cloned();
    let stream = stream.to_owned();
    tokio::task::spawn_blocking(move || {
        output::page_for_operation(&store, &operation, prepared.as_ref(), &stream, offset)
    })
    .await
    .map_err(problem)?
}

fn prepare(
    store: &StoreBinding,
    owner: &OperationOwner,
    repository: &Path,
    args: &[String],
) -> Result<Request> {
    let command = Descriptor { repository: repository.to_owned(), args: args.to_vec() };
    let descriptor = descriptor_digest(&command)?;
    let operation_digest = digest(owner.identity().as_bytes());
    let request = Request {
        schema_version: SCHEMA_VERSION,
        workspace: store.workspace.clone(),
        store: store.identity.clone(),
        state_file: store.state_file.clone(),
        operation: owner.identity().to_owned(),
        descriptor: descriptor.clone(),
        job_name: job_name(store, &operation_digest),
        command,
    };
    validate_request(store, &request, None)?;
    validate_repository(&request.command.repository)?;
    owner.retain_prepared(prepared_receipt(&request))?;
    let directory = store.directory(owner.identity());
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("request.json");
    if let Some(existing) = read_optional::<Request>(&path)? {
        if existing != request {
            return Err(problem("The original Git operation owns a different command receipt"));
        }
    } else {
        save_json(&path, &request)?;
    }
    Ok(request)
}

pub(super) fn prepared_receipt(request: &Request) -> Value {
    json!({
        "kind":"peritus-web-git-effect",
        "schemaVersion":SCHEMA_VERSION,
        "workspace":request.workspace,
        "store":request.store,
        "stateFile":request.state_file,
        "descriptor":request.descriptor,
        "effect":digest(request.operation.as_bytes())
    })
}

pub(super) fn validate_request(
    store: &StoreBinding,
    request: &Request,
    prepared: Option<&Value>,
) -> Result<()> {
    if request.schema_version != SCHEMA_VERSION
        || request.workspace != store.workspace
        || request.store != store.identity
        || request.state_file != store.state_file
        || request.job_name != job_name(store, &digest(request.operation.as_bytes()))
        || request.operation.is_empty()
        || !request.operation.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        || request.command.args.is_empty()
        || request.command.args.iter().any(|arg| arg.contains('\0'))
        || descriptor_digest(&request.command)? != request.descriptor
        || prepared.is_some_and(|value| value != &prepared_receipt(request))
    {
        return Err(uncertain("The durable Git request does not match its workspace receipt"));
    }
    Ok(())
}

fn job_name(store: &StoreBinding, operation_digest: &str) -> String {
    format!("Local\\PeritusGit-{}-{operation_digest}", store.identity)
}

pub(super) fn validate_state(request: &Request, state: &EffectState) -> Result<()> {
    if state.schema_version != SCHEMA_VERSION
        || state.workspace != request.workspace
        || state.store != request.store
        || state.operation != request.operation
        || state.descriptor != request.descriptor
        || state.owner.pid == 0
        || state.owner.start_token == 0
        || (state.phase == Phase::Completed) != state.result.is_some()
    {
        return Err(uncertain("The durable Git effect record does not match its command receipt"));
    }
    platform::validate_owner_binding(&state.owner, &request.job_name)?;
    Ok(())
}

pub(super) fn validate_repository(repository: &Path) -> Result<()> {
    if repository.canonicalize()? != repository || !repository.is_dir() {
        return Err(problem("The durable Git repository binding is no longer canonical"));
    }
    Ok(())
}

fn launch_owner(directory: &Path) -> Result<tokio::process::Child> {
    let executable = std::env::current_exe()?;
    let mut command = tokio::process::Command::new(executable);
    command
        .arg(OWNER_FLAG)
        .arg(directory.join("request.json"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    platform::configure_owner_command(&mut command);
    Ok(command.spawn()?)
}

pub(super) fn observe(binding: &OwnerBinding) -> Result<ProbeObservation> {
    let mut probe = NativeProcessProbe::new();
    probe.observe(binding.identity()).map_err(problem)
}

pub(super) fn read_state(directory: &Path) -> Result<Option<EffectState>> {
    read_optional(&directory.join("state.json"))
}

pub(super) fn read_optional<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Option<T>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub(super) fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}

pub(super) fn save_json(path: &Path, value: &impl Serialize) -> Result<()> {
    save(path, &serde_json::to_vec(value)?)
}

fn descriptor_digest(value: &Descriptor) -> Result<String> {
    Ok(digest(&serde_json::to_vec(value)?))
}

pub(super) fn digest(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}
