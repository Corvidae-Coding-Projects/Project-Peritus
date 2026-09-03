//! Shared C4/C2 command ownership for product developer tools.

mod authority;
mod contract;
mod control;
mod identity;
mod journal;
mod kernel;
mod lease;
mod plan;
mod result;
mod sandbox;

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use peritus_agent::DeveloperLoopError;
use peritus_artifact_store::{ArtifactStore, StoreConfig};
use peritus_policy::{AuthorityInstant, OperationDescriptor, OperationRegistry, RiskSet};
use peritus_process::{ExecutionGateway, ProcessStore};
use peritus_tool_protocol::{CancellationReason, ToolControl, ToolProgress, ToolResult};
use peritus_tool_router::{
    DispatchOutcome, InvocationHandle, RecoveryOutcome, RouterLimits, ToolRegistry, ToolRouter,
};
use peritus_tools_shell::{RawShellDispatcher, exec_descriptor};
use peritus_types::RunId;
use serde_json::Value;

use super::path::{canonical_command_cwd, tool};

const ARTIFACT_QUOTA_BYTES: u64 = 1024 * 1_024 * 1_024;
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// Cloneable handle to one run-owned active command table and durable C2 process store.
#[derive(Clone)]
pub struct CommandRuntime {
    inner: Arc<RuntimeInner>,
}

struct RuntimeInner {
    run_id: RunId,
    workspace_root: PathBuf,
    state_root: PathBuf,
    artifacts: StoreConfig,
    gateway: ExecutionGateway,
    state: Mutex<RuntimeState>,
    #[cfg(test)]
    #[allow(dead_code, reason = "keeps the temporary command registry alive for unit tests")]
    state_guard: Option<tempfile::TempDir>,
}

struct RuntimeState {
    router: ToolRouter,
    next_ordinal: u64,
    active: BTreeMap<String, ActiveCommand>,
    terminal: BTreeMap<String, TerminalCommand>,
}

struct ActiveCommand {
    invocation: InvocationHandle,
    started: Instant,
    interactive: bool,
}

struct TerminalCommand {
    result: ToolResult,
    progress: Vec<ToolProgress>,
}

/// Fully checked input for one command start.
pub(super) struct StartCommand<'a> {
    pub(super) program: &'a str,
    pub(super) arguments: &'a [String],
    pub(super) cwd: &'a Path,
    pub(super) timeout: Duration,
    pub(super) interactive: bool,
    pub(super) rows: u16,
    pub(super) columns: u16,
    pub(super) idempotency_key: &'a str,
    pub(super) environment: Vec<(String, String)>,
}

impl CommandRuntime {
    /// Creates the run-owned C4 router while reusing the caller's daemon-owned C2 process store.
    ///
    /// # Errors
    /// Returns a product-run failure when the state root overlaps the agent-visible workspace or
    /// the canonical C4 catalog cannot be constructed.
    pub fn open(
        state_root: impl Into<PathBuf>,
        workspace_root: impl Into<PathBuf>,
        run_id: RunId,
        process_store: ProcessStore,
    ) -> Result<Self, crate::ProductRunnerError> {
        let state_root = state_root.into();
        let workspace_root = workspace_root.into();
        std::fs::create_dir_all(&state_root).map_err(|error| runtime_open(error.to_string()))?;
        let state_root =
            state_root.canonicalize().map_err(|error| runtime_open(error.to_string()))?;
        let workspace_root =
            workspace_root.canonicalize().map_err(|error| runtime_open(error.to_string()))?;
        if state_root.starts_with(&workspace_root) || workspace_root.starts_with(&state_root) {
            return Err(runtime_open(
                "command state and agent-visible workspace roots overlap".to_owned(),
            ));
        }
        let artifacts = StoreConfig::new(
            state_root.join("artifacts"),
            plan::OUTPUT_BYTES,
            ARTIFACT_QUOTA_BYTES,
        )
        .map_err(|error| runtime_open(error.to_string()))?;
        let descriptor = exec_descriptor().map_err(|error| runtime_open(error.to_string()))?;
        let operation = OperationDescriptor::new(
            descriptor.operation().name().clone(),
            descriptor.operation().operation_class(),
            RiskSet::new(descriptor.operation().risks().as_slice().to_vec())
                .map_err(|error| runtime_open(format!("{error:?}")))?,
        )
        .map_err(|error| runtime_open(format!("{error:?}")))?;
        let operations = OperationRegistry::new(vec![operation])
            .map_err(|error| runtime_open(format!("{error:?}")))?;
        let registry = ToolRegistry::new(vec![Arc::new(descriptor)], &operations)
            .map_err(|error| runtime_open(error.to_string()))?;
        let limits =
            RouterLimits::new(64, 4_096).map_err(|error| runtime_open(error.to_string()))?;
        Ok(Self {
            inner: Arc::new(RuntimeInner {
                run_id,
                workspace_root,
                state_root,
                artifacts,
                gateway: ExecutionGateway::new(process_store),
                state: Mutex::new(RuntimeState {
                    router: ToolRouter::new(registry, limits),
                    next_ordinal: 0,
                    active: BTreeMap::new(),
                    terminal: BTreeMap::new(),
                }),
                #[cfg(test)]
                state_guard: None,
            }),
        })
    }

    #[cfg(test)]
    pub(crate) fn open_for_test(workspace_root: &Path, run_id: RunId) -> Self {
        let state_guard = tempfile::tempdir().expect("temporary command state");
        let processes = ProcessStore::open(state_guard.path().join("processes"), workspace_root)
            .expect("test command process store");
        let mut runtime =
            Self::open(state_guard.path().join("router"), workspace_root, run_id, processes)
                .expect("test command runtime");
        Arc::get_mut(&mut runtime.inner).expect("new command runtime is unique").state_guard =
            Some(state_guard);
        runtime
    }

    pub(super) fn start(&self, request: StartCommand<'_>) -> Result<Value, DeveloperLoopError> {
        let handle = self.start_owned(request)?;
        Ok(result::active(&handle, &[]))
    }

    pub(super) fn run(&self, request: StartCommand<'_>) -> Result<Value, DeveloperLoopError> {
        let handle = self.start_owned(request)?;
        loop {
            let observation = self.poll(&handle)?;
            if observation.get("state").and_then(Value::as_str) != Some("running") {
                return Ok(observation);
            }
            thread::sleep(POLL_INTERVAL);
        }
    }

    pub(super) fn poll(&self, handle: &str) -> Result<Value, DeveloperLoopError> {
        self.observe(handle, Observation::Poll)
    }

    fn start_owned(&self, request: StartCommand<'_>) -> Result<String, DeveloperLoopError> {
        let cwd = canonical_command_cwd(&self.inner.workspace_root, request.cwd)?;
        let timeout_millis =
            u64::try_from(request.timeout.as_millis()).unwrap_or(u64::MAX).clamp(1, 600_000);
        let mut state = self.inner.state.lock().map_err(|_| tool("command runtime is poisoned"))?;
        state.next_ordinal = state
            .next_ordinal
            .checked_add(1)
            .ok_or_else(|| tool("command runtime action ordinal overflowed"))?;
        let ordinal = state.next_ordinal;
        let contract = contract::command_contract(self.inner.run_id, ordinal).map_err(tool)?;
        let ids = identity::CommandIds::new(self.inner.run_id, ordinal, &contract).map_err(tool)?;
        let command = plan::compile(
            &state.router,
            &ids,
            plan::CommandRequest {
                program: request.program,
                arguments: request.arguments,
                cwd: &cwd,
                timeout_millis,
                interactive: request.interactive,
                rows: request.rows,
                columns: request.columns,
                idempotency_key: identity::bounded_key(request.idempotency_key),
                environment: request.environment,
            },
        )
        .map_err(tool)?;
        let authority_root =
            self.inner.state_root.join("authority").join(identity::action_hex(ids.action));
        let tool_authority = authority::commit_tool(
            &authority_root.join("c4.sqlite3"),
            &ids,
            &contract,
            &command.prepared,
            timeout_millis,
        )
        .map_err(tool)?;
        let process_authority = authority::commit_process(
            &authority_root.join("c2.sqlite3"),
            &ids,
            &contract,
            &command.execution,
            timeout_millis,
        )
        .map_err(tool)?;
        let process_request = process_authority.request(&ids, &command.execution);
        let artifacts = ArtifactStore::open(self.inner.artifacts.clone())
            .map_err(|error| tool(error.to_string()))?;
        let mut dispatcher = RawShellDispatcher::new(
            &self.inner.gateway,
            &process_request,
            command.execution,
            artifacts,
        )
        .map_err(|error| tool(error.to_string()))?;
        let tool_request = tool_authority.request(&ids, &command.prepared);
        let outcome = state
            .router
            .dispatch(command.prepared, &tool_request, &mut dispatcher)
            .map_err(|error| tool(error.to_string()))?;
        let handle = identity::action_hex(ids.action);
        match outcome {
            DispatchOutcome::Active(invocation) => {
                state.active.insert(
                    handle.clone(),
                    ActiveCommand {
                        invocation,
                        started: Instant::now(),
                        interactive: request.interactive,
                    },
                );
            }
            DispatchOutcome::Completed(result) | DispatchOutcome::Replayed(result) => {
                state
                    .terminal
                    .insert(handle.clone(), TerminalCommand { result, progress: Vec::new() });
            }
            DispatchOutcome::PriorOutcome(disposition) => {
                return Err(tool(format!(
                    "command has prior non-replayable outcome: {disposition:?}"
                )));
            }
        }
        drop(state);
        Ok(handle)
    }

    fn observe(&self, handle: &str, operation: Observation) -> Result<Value, DeveloperLoopError> {
        let mut state = self.inner.state.lock().map_err(|_| tool("command runtime is poisoned"))?;
        if let Some(terminal) = state.terminal.get(handle) {
            return result::terminal(
                handle,
                &terminal.result,
                &self.inner.artifacts,
                &terminal.progress,
            )
            .map_err(tool);
        }
        let (invocation, observed_at) = {
            let active = state
                .active
                .get(handle)
                .ok_or_else(|| tool("command invocation handle is unknown"))?;
            (active.invocation, observed_at(active.started))
        };
        match operation {
            Observation::Recover => match state.router.recover(invocation, observed_at) {
                Ok(RecoveryOutcome::Active(update)) => {
                    Ok(result::active(handle, update.progress()))
                }
                Ok(RecoveryOutcome::Completed(terminal)) => {
                    state.active.remove(handle);
                    let value = result::terminal(handle, &terminal, &self.inner.artifacts, &[])
                        .map_err(tool)?;
                    state.terminal.insert(
                        handle.to_owned(),
                        TerminalCommand { result: terminal, progress: Vec::new() },
                    );
                    Ok(value)
                }
                Ok(RecoveryOutcome::Indeterminate(failure)) => {
                    state.active.remove(handle);
                    Ok(result::indeterminate(handle, failure.failure().detail().as_str()))
                }
                Err(error) => Err(tool(error.to_string())),
            },
            operation => {
                let update = match operation {
                    Observation::Poll => state.router.poll(invocation, observed_at),
                    Observation::Control(control) => {
                        state.router.control(invocation, control, observed_at)
                    }
                    Observation::Cancel => {
                        state.router.cancel(invocation, CancellationReason::Requested, observed_at)
                    }
                    Observation::Recover => {
                        return Err(tool("command recovery reached the ordinary observation path"));
                    }
                }
                .map_err(|error| tool(error.to_string()))?;
                self.accept_update(&mut state, handle, update.progress(), update.terminal())
            }
        }
    }

    fn accept_update(
        &self,
        state: &mut RuntimeState,
        handle: &str,
        progress: &[ToolProgress],
        terminal: Option<&ToolResult>,
    ) -> Result<Value, DeveloperLoopError> {
        let Some(terminal) = terminal.cloned() else {
            return Ok(result::active(handle, progress));
        };
        let value =
            result::terminal(handle, &terminal, &self.inner.artifacts, progress).map_err(tool)?;
        state.active.remove(handle);
        state.terminal.insert(
            handle.to_owned(),
            TerminalCommand { result: terminal, progress: progress.to_vec() },
        );
        Ok(value)
    }
}

enum Observation {
    Poll,
    Control(ToolControl),
    Cancel,
    Recover,
}

fn observed_at(started: Instant) -> AuthorityInstant {
    let elapsed = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    AuthorityInstant::new(peritus_types::Generation::first(), 20_u64.saturating_add(elapsed))
}

fn runtime_open(detail: String) -> crate::ProductRunnerError {
    crate::ProductRunnerError::new(
        crate::ProductRunnerErrorKind::Apply,
        "open product command runtime",
        detail,
    )
}

#[cfg(test)]
mod tests;
