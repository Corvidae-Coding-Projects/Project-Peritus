//! C4 call and caller-bound C2 execution-plan construction.

use std::path::{Path, PathBuf};

use peritus_policy::AuthorityInstant;
#[cfg(not(windows))]
use peritus_process::TerminalSize;
use peritus_process::{
    CommandSpec, DeadlinePolicy, EnvironmentPlan, EnvironmentVariable, ExecutionCallerBinding,
    ExecutionCallerTarget, ExecutionPlan, GracefulAction, IoMode, OutputOverflowAction,
    OutputPolicy, ProcessResourcePolicy, StdinPolicy, WorkingDirectory, WorkspaceAccess,
};
use peritus_tool_protocol::{
    BoundedJson, CallLimits, IdempotencyKey, JsonLimits, PreparedToolCall, SemanticVersion,
    ToolCall,
};
use peritus_tool_router::ToolRouter;
use serde_json::Value;

use super::{identity::CommandIds, sandbox};
use crate::developer_tools::wire::object;

pub(super) const OUTPUT_BYTES: u64 = 8 * 1_024 * 1_024;
pub(super) const MODEL_OUTPUT_BYTES: u32 = 16 * 1_024;
const MEMORY_BYTES: u64 = 12 * 1_024 * 1_024 * 1_024;
const DISK_BYTES: u64 = 50 * 1_024 * 1_024 * 1_024;
const PROCESS_COUNT: u64 = 4_096;
const FILE_DESCRIPTORS: u64 = 8_192;

pub(super) struct CommandPlan {
    pub(super) prepared: PreparedToolCall,
    pub(super) execution: ExecutionPlan,
}

pub(super) struct CommandRequest<'a> {
    pub(super) program: &'a str,
    pub(super) arguments: &'a [String],
    pub(super) cwd: &'a Path,
    pub(super) timeout_millis: u64,
    pub(super) interactive: bool,
    pub(super) rows: u16,
    pub(super) columns: u16,
    pub(super) idempotency_key: String,
    pub(super) environment: Vec<(String, String)>,
}

#[allow(
    clippy::too_many_lines,
    reason = "one command compiler keeps the C4 call and exact caller-bound C2 plan visibly aligned"
)]
pub(super) fn compile(
    router: &ToolRouter,
    ids: &CommandIds,
    request: CommandRequest<'_>,
) -> Result<CommandPlan, String> {
    let executable = resolve_executable(request.program, request.cwd)?;
    let wire_arguments = object(vec![
        ("arguments", Value::Array(request.arguments.iter().cloned().map(Value::String).collect())),
        ("executable", Value::String(executable.clone())),
    ]);
    let arguments = BoundedJson::parse(&wire_arguments.to_string(), JsonLimits::PRODUCTION)
        .map_err(|error| format!("encode command tool arguments: {error}"))?;
    let limits = CallLimits::new(
        request.timeout_millis,
        OUTPUT_BYTES,
        MODEL_OUTPUT_BYTES,
        MODEL_OUTPUT_BYTES,
        4_096,
        3,
    )
    .map_err(|error| format!("construct command call limits: {error}"))?;
    let call = ToolCall::new(
        ids.action,
        ids.capability.clone(),
        SemanticVersion::new(1, 0, 0)
            .map_err(|error| format!("construct command tool version: {error}"))?,
        arguments,
        limits,
        ids.revision,
        AuthorityInstant::new(
            peritus_types::Generation::first(),
            request.timeout_millis.saturating_add(21),
        ),
        IdempotencyKey::new(request.idempotency_key)
            .map_err(|error| format!("construct command idempotency key: {error}"))?,
    );
    let prepared =
        router.prepare(call).map_err(|error| format!("prepare command call: {error}"))?;
    let environment = environment(request.environment)?;
    #[cfg(windows)]
    let io = {
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
        StdinPolicy::bounded(65_536, 16 * 1_024 * 1_024)
            .map_err(|error| format!("construct command stdin policy: {error}"))?
    } else {
        StdinPolicy::Closed
    };
    let output = OutputPolicy::new(
        16 * 1_024,
        512 * 1_024,
        OUTPUT_BYTES,
        16_384,
        OUTPUT_BYTES,
        OUTPUT_BYTES,
        OUTPUT_BYTES,
        OutputOverflowAction::ContinueIncomplete,
    )
    .map_err(|error| format!("construct command output policy: {error}"))?;
    let resources = ProcessResourcePolicy::new(
        request.timeout_millis,
        request.timeout_millis.saturating_mul(64),
        MEMORY_BYTES,
        DISK_BYTES,
        OUTPUT_BYTES,
        PROCESS_COUNT,
        FILE_DESCRIPTORS,
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
        WorkspaceAccess::Writable,
    )
    .map_err(|error| format!("open command working directory: {error}"))?;
    let (checked, admission) =
        sandbox::raw_effect(ids, &executable, request.cwd, &environment, io, stdin, resources)?;
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
        CommandSpec::new(executable, request.arguments.iter().cloned())
            .map_err(|error| format!("construct structured command: {error}"))?,
        working_directory,
        environment,
        io,
        stdin,
        output,
        DeadlinePolicy::new(Some(request.timeout_millis), GracefulAction::Terminate, 500, 5_000)
            .map_err(|error| format!("construct command deadline policy: {error}"))?,
        resources,
        &checked,
        &admission,
    )
    .and_then(|plan| plan.bind_caller(caller))
    .map_err(|error| format!("compile caller-bound command plan: {error}"))?;
    Ok(CommandPlan { prepared, execution })
}

fn environment(bindings: Vec<(String, String)>) -> Result<EnvironmentPlan, String> {
    let mut allowlist = std::env::vars_os()
        .filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)))
        .filter(|(name, _)| valid_environment_name(name))
        .map(|(name, _)| name)
        .collect::<Vec<_>>();
    allowlist.sort_by_key(|name| name.to_ascii_uppercase());
    allowlist.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
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

fn valid_environment_name(name: &str) -> bool {
    !name.is_empty()
        && name.bytes().enumerate().all(|(index, byte)| {
            byte == b'_' || byte.is_ascii_alphabetic() || (index > 0 && byte.is_ascii_digit())
        })
}

fn resolve_executable(program: &str, cwd: &Path) -> Result<String, String> {
    let path = Path::new(program);
    let candidates = if path.is_absolute() {
        vec![path.to_path_buf()]
    } else if path.components().count() > 1 {
        vec![cwd.join(path)]
    } else {
        executable_candidates(program)
    };
    for candidate in candidates {
        if candidate.is_file()
            && let Ok(canonical) = candidate.canonicalize()
            && let Some(text) = canonical.to_str()
        {
            return Ok(text.to_owned());
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
            extensions.iter().map(move |extension| directory.join(format!("{program}{extension}")))
        })
        .collect()
}

fn executable_extensions(program: &str) -> Vec<String> {
    #[cfg(windows)]
    {
        if Path::new(program).extension().is_some() {
            return vec![String::new()];
        }
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_owned())
            .split(';')
            .map(str::to_ascii_lowercase)
            .chain(std::iter::once(String::new()))
            .collect()
    }
    #[cfg(not(windows))]
    {
        let _ = program;
        vec![String::new()]
    }
}
