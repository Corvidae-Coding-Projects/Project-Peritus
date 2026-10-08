//! C4 call and caller-bound C2 execution-plan construction.

use std::{ffi::OsString, path::{Path, PathBuf}};

use peritus_policy::AuthorityInstant;
#[cfg(not(windows))]
use peritus_process::TerminalSize;
use peritus_process::{
    CommandSpec, DeadlinePolicy, EnvironmentPlan, EnvironmentVariable, ExecutionCallerBinding,
    ExecutionCallerTarget, ExecutionPlan, IoMode, OutputPolicy, ProcessResourcePolicy,
    StdinPolicy, WorkingDirectory, WorkspaceAccess, native_executable_reference,
};
use peritus_provider_core::CancellationToken;
use peritus_sandbox::{BackendAdmission, CheckedSandboxPlan};
use peritus_tool_protocol::{
    BoundedJson, CallLimits, IdempotencyKey, JsonLimits, PreparedToolCall, SemanticVersion,
    ToolCall,
};
use peritus_tool_router::ToolRegistry;
use serde_json::Value;

use super::{CommandExecutionMode, identity::CommandIds, native_gate, sandbox};
use crate::developer_tools::wire::object;

#[cfg(test)]
mod tests;

const OUTPUT_SEGMENT_BYTES: u64 = 8 * 1_024 * 1_024;
pub(super) const MODEL_OUTPUT_BYTES: u32 = 16 * 1_024;

pub(super) struct CommandPlan {
    pub(super) prepared: PreparedToolCall,
    pub(super) execution: ExecutionPlan,
    pub(super) backend: CommandBackend,
}

pub(super) enum CommandBackend {
    Raw {
        checked: CheckedSandboxPlan,
        admission: BackendAdmission,
    },
    Native(native_gate::PreparedNativeGate),
}

impl CommandBackend {
    fn sandbox(&self) -> (&CheckedSandboxPlan, &BackendAdmission) {
        match self {
            Self::Raw { checked, admission } => (checked, admission),
            Self::Native(prepared) => (&prepared.checked, &prepared.admission),
        }
    }
}

pub(super) struct CommandRequest<'a> {
    pub(super) program: &'a str,
    pub(super) arguments: &'a [String],
    pub(super) cwd: &'a Path,
    pub(super) timeout_millis: Option<u64>,
    pub(super) interactive: bool,
    pub(super) rows: u16,
    pub(super) columns: u16,
    pub(super) idempotency_key: String,
    pub(super) environment: Vec<(String, String)>,
    pub(super) mode: CommandExecutionMode,
    pub(super) protected_paths: &'a [PathBuf],
}

#[allow(
    clippy::too_many_lines,
    reason = "one command compiler keeps the C4 call and exact caller-bound C2 plan visibly aligned"
)]
pub(super) fn compile(
    registry: &ToolRegistry,
    ids: &CommandIds,
    workspace_root: &Path,
    state_root: &Path,
    cancellation: &CancellationToken,
    request: CommandRequest<'_>,
) -> Result<CommandPlan, String> {
    let executable = resolve_executable(request.program, request.cwd)?;
    let wire_executable = executable
        .to_str()
        .map(str::to_owned)
        .unwrap_or_else(|| native_executable_reference(executable.as_os_str()));
    let wire_arguments = object(vec![
        ("arguments", Value::Array(request.arguments.iter().cloned().map(Value::String).collect())),
        ("executable", Value::String(wire_executable)),
    ]);
    let arguments = BoundedJson::parse(&wire_arguments.to_string(), JsonLimits::PRODUCTION)
        .map_err(|error| format!("encode command tool arguments: {error}"))?;
    let limits = CallLimits::with_optional_output(
        request.timeout_millis,
        None,
        MODEL_OUTPUT_BYTES,
        MODEL_OUTPUT_BYTES,
        4_096,
        3,
    )
    .map_err(|error| format!("construct command call limits: {error}"))?
    .with_paged_progress();
    let call = ToolCall::new(
        ids.action,
        ids.capability.clone(),
        SemanticVersion::new(2, 0, 0)
            .map_err(|error| format!("construct command tool version: {error}"))?,
        arguments,
        limits,
        ids.revision,
        AuthorityInstant::new(
            peritus_types::Generation::first(),
            request.timeout_millis.map_or(21, |timeout| timeout.saturating_add(21)),
        ),
        IdempotencyKey::new(request.idempotency_key)
            .map_err(|error| format!("construct command idempotency key: {error}"))?,
    );
    let prepared =
        registry.prepare(call).map_err(|error| format!("prepare command call: {error}"))?;
    let environment = match request.mode {
        CommandExecutionMode::Observational => {
            native_gate::observational_environment(request.program, request.environment)?
        }
        CommandExecutionMode::Mutation => environment(request.environment)?,
    };
    #[cfg(windows)]
    let io = if request.mode.is_observational() && request.interactive {
        IoMode::Pty(
            peritus_process::TerminalSize::new(request.rows, request.columns, 0, 0)
                .map_err(|error| format!("construct command PTY size: {error}"))?,
        )
    } else {
        // Raw C2 launches cannot supply a contained ConPTY session. Keep interactive input
        // functional through bounded pipes; restricted daemon launches retain native ConPTY.
        let _ = (request.rows, request.columns);
        IoMode::Pipes
    };
    #[cfg(not(windows))]
    let io = if request.interactive {
        IoMode::Pty(
            TerminalSize::new(request.rows, request.columns, 0, 0)
                .map_err(|error| format!("construct command PTY size: {error}"))?,
        )
    } else {
        IoMode::Pipes
    };
    let stdin = if request.interactive {
        StdinPolicy::streaming(65_536)
            .map_err(|error| format!("construct command stdin policy: {error}"))?
    } else {
        StdinPolicy::Closed
    };
    let output = OutputPolicy::streaming(
        16 * 1_024,
        512 * 1_024,
        OUTPUT_SEGMENT_BYTES,
        16_384,
    )
    .map_err(|error| format!("construct command output policy: {error}"))?;
    let resources = ProcessResourcePolicy::with_optional_limits(
        request.timeout_millis,
        None,
        None,
        None,
        None,
        None,
        None,
        1,
    )
    .map_err(|error| format!("construct command resource policy: {error}"))?;
    let working_directory = WorkingDirectory::open(
        request.cwd,
        ids.workspace,
        ids.resource,
        ids.environment,
        ids.revision.workspace_generation(),
        ids.revision.workspace_revision(),
        match request.mode {
            CommandExecutionMode::Observational => WorkspaceAccess::ReadOnly,
            CommandExecutionMode::Mutation => WorkspaceAccess::Writable,
        },
    )
    .map_err(|error| format!("open command working directory: {error}"))?;
    let command = CommandSpec::new(executable, request.arguments.iter().cloned())
        .map_err(|error| format!("construct structured command: {error}"))?;
    let backend = match request.mode {
        CommandExecutionMode::Observational => CommandBackend::Native(
            native_gate::prepare_observational(
                ids,
                &command,
                workspace_root,
                working_directory.path(),
                &environment,
                io,
                stdin,
                resources,
                state_root,
                cancellation,
            )?,
        ),
        CommandExecutionMode::Mutation => {
            if request.protected_paths.is_empty() {
                let (checked, admission) = sandbox::raw_effect(
                    ids,
                    &command,
                    working_directory.path(),
                    &environment,
                    io,
                    stdin,
                    resources,
                )?;
                CommandBackend::Raw { checked, admission }
            } else {
                CommandBackend::Native(native_gate::prepare_confined_mutation(
                    ids,
                    &command,
                    workspace_root,
                    working_directory.path(),
                    &environment,
                    io,
                    stdin,
                    resources,
                    request.protected_paths,
                    state_root,
                    cancellation,
                )?)
            }
        }
    };
    let (checked, admission) = backend.sandbox();
    let caller = ExecutionCallerBinding::new(
        ids.action,
        ids.capability.clone(),
        prepared.descriptor_digest().get(),
        prepared.prepared_digest(),
        ExecutionCallerTarget::new(
            ids.actor,
            peritus_policy::ActorRole::ProviderToolWorker,
            ids.environment,
            ids.resource,
        ),
    );
    let execution = ExecutionPlan::new(
        ids.execution_identity(),
        command,
        working_directory,
        environment,
        io,
        stdin,
        output,
        DeadlinePolicy::force(request.timeout_millis)
            .map_err(|error| format!("construct command deadline policy: {error}"))?,
        resources,
        &checked,
        &admission,
    )
    .and_then(|plan| plan.bind_caller(caller))
    .map_err(|error| format!("compile caller-bound command plan: {error}"))?;
    Ok(CommandPlan { prepared, execution, backend })
}

fn environment(bindings: Vec<(String, String)>) -> Result<EnvironmentPlan, String> {
    let allowlist = std::env::vars_os()
        .map(|(name, _)| name)
        .filter(|name| EnvironmentVariable::new(name.clone(), OsString::new()).is_ok())
        .collect::<Vec<_>>();
    let bindings = bindings
        .into_iter()
        .map(|(name, value)| {
            EnvironmentVariable::new(name, value)
                .map_err(|error| format!("construct command environment binding: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    EnvironmentPlan::allowlisted(allowlist, bindings)
        .map_err(|error| format!("snapshot command environment: {error}"))
}

fn resolve_executable(program: &str, cwd: &Path) -> Result<PathBuf, String> {
    let path = Path::new(program);
    let candidates = if path.is_absolute() {
        vec![path.to_path_buf()]
    } else if path.components().count() > 1 {
        vec![cwd.join(path)]
    } else {
        executable_candidates(program)
    };
    for candidate in candidates {
        let candidate = if candidate.is_absolute() { candidate } else { cwd.join(candidate) };
        if candidate.is_file()
            && let Some(parent) = candidate.parent()
            && let Ok(parent) = parent.canonicalize()
            && let Some(name) = candidate.file_name()
        {
            // Keep the final executable name. Multicall launchers such as rustup and ccache
            // dispatch by argv[0]; resolving cargo's final symlink would execute rustup instead.
            // The exact same absolute path is used in both authorization and execution.
            return Ok(parent.join(name));
        }
    }
    Err(format!(
        "executable `{program}` was not found through PATH; verify the requested program or path, then inspect available package or runtime managers. When the active task and environment authorize ordinary dependency installation, install the prerequisite and retry the real command; otherwise report this exact missing prerequisite. Do not substitute a stand-in for the requested deliverable"
    ))
}

fn executable_candidates(program: &str) -> Vec<PathBuf> {
    let extensions = executable_extensions(program);
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .flat_map(|directory| {
            extensions.iter().map(move |extension| {
                let mut file = OsString::from(program);
                file.push(extension);
                directory.join(file)
            })
        })
        .collect()
}

fn executable_extensions(program: &str) -> Vec<OsString> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::{OsStrExt as _, OsStringExt as _};

        if Path::new(program).extension().is_some() {
            return vec![OsString::new()];
        }
        let value = std::env::var_os("PATHEXT")
            .unwrap_or_else(|| OsString::from(".COM;.EXE;.BAT;.CMD"));
        let units = value.encode_wide().collect::<Vec<_>>();
        let mut extensions = units
            .split(|unit| *unit == u16::from(b';'))
            .filter(|extension| !extension.is_empty())
            .map(OsString::from_wide)
            .collect::<Vec<_>>();
        extensions.push(OsString::new());
        extensions
    }
    #[cfg(not(windows))]
    {
        let _ = program;
        vec![OsString::new()]
    }
}
