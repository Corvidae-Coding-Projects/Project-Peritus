//! Shared C4/C2 command ownership for product developer tools.

mod authority;
mod cancellation;
mod compactor;
pub(crate) use compactor::PreparedLocalCompactor;
mod construction;
mod contract;
mod control;
mod folder_patch;
mod gate_runtime;
mod identity;
mod journal;
mod kernel;
mod lease;
mod native_gate;
pub(crate) use native_gate::{GateInvocationOutcome, GateInvocationRequest};
mod network;
pub use network::{
    ManagedGateNetworkCatalog, ManagedGateNetworkDestination, ManagedGateNetworkGrant,
};
mod ordinal;
mod plan;
mod preview;
mod preview_terminal;
pub use preview_terminal::PreviewTerminal;
mod projections;
mod replay_index;
mod result;
mod sandbox;

pub use folder_patch::{FolderPatchAuthority, FolderPatchAuthorityPlan};

use std::{
    collections::{BTreeMap, btree_map::Entry},
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use peritus_agent::DeveloperLoopError;
use peritus_artifact_store::{ArtifactStore, StoreConfig};
use peritus_policy::AuthorityInstant;
use peritus_process::{ExecutionGateway, WorkspaceAccess};
use peritus_provider_core::CancellationToken;
use peritus_tool_protocol::{
    CancellationReason, ImplementationIdentity, SchemaDigest, ToolControl, ToolProgress, ToolResult,
};
use peritus_tool_router::{
    AuthorizedInvocation, DispatchFailure, DispatchOutcome, ExecutionUpdate, InterruptedDispatch,
    InvocationHandle, ProgressPage, RecoveryOutcome, RouterError, RouterLimits, ToolDispatcher,
    ToolRegistry, ToolRouter, ToolStart,
};
use peritus_tools_shell::{RawShellDispatcher, ShellDispatcher};
use peritus_types::{ActionId, RunId};
use serde_json::Value;

use super::path::{canonical_command_cwd, tool};

const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// Cloneable handle to one run-owned active command table and durable C2 process store.
#[derive(Clone)]
pub struct CommandRuntime {
    inner: Arc<RuntimeInner>,
    local_context: crate::LocalContextConfig,
    managed_gate_network: ManagedGateNetworkCatalog,
    managed_gate_workspace: Option<peritus_types::WorkspaceId>,
}

struct RuntimeInner {
    run_id: RunId,
    workspace_root: PathBuf,
    state_root: PathBuf,
    artifacts: StoreConfig,
    registry: ToolRegistry,
    router_limits: RouterLimits,
    replay_index: replay_index::ReplayIndex,
    cancellation_worker: cancellation::CancellationWorker,
    gateway: ExecutionGateway,
    projection_lock: Mutex<()>,
    state: Mutex<RuntimeState>,
    #[cfg(test)]
    #[allow(dead_code, reason = "keeps the temporary command registry alive for unit tests")]
    state_guard: Option<tempfile::TempDir>,
}

struct RuntimeState {
    next_ordinal: u64,
    next_folder_patch_ordinal: u64,
    starting: BTreeMap<String, StartingCommand>,
    active: BTreeMap<String, ActiveCommand>,
    terminal: BTreeMap<String, TerminalCommand>,
    recovered: BTreeMap<String, Value>,
    replay_pending: BTreeMap<ActionId, CommandRouter>,
}

type CommandRouter = Arc<Mutex<ToolRouter>>;

struct StartingCommand {
    plan: peritus_process::ExecutionPlan,
    router: CommandRouter,
    started: Instant,
    interactive: bool,
    resource_evidence: Option<Value>,
    protected_paths: Vec<PathBuf>,
}

struct ActiveCommand {
    plan: peritus_process::ExecutionPlan,
    control: Option<peritus_process::ProcessControl>,
    router: CommandRouter,
    invocation: InvocationHandle,
    started: Instant,
    interactive: bool,
    resource_evidence: Option<Value>,
    protected_paths: Vec<PathBuf>,
}

#[derive(Clone)]
struct TerminalCommand {
    result: ToolResult,
    progress: ProgressBatch,
    mode: CommandExecutionMode,
    resource_evidence: Option<Value>,
}

struct ProjectionContext {
    mode: Option<CommandExecutionMode>,
    resource_evidence: Option<Value>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CommandExecutionMode {
    Observational,
    Mutation,
}

impl CommandExecutionMode {
    pub(super) const fn from_purpose(purpose: &str) -> Option<Self> {
        match purpose.as_bytes() {
            b"verification" => Some(Self::Observational),
            b"external_effect" => Some(Self::Mutation),
            _ => None,
        }
    }

    pub(super) const fn is_observational(self) -> bool {
        matches!(self, Self::Observational)
    }

    pub(super) const fn is_mutation(self) -> bool {
        matches!(self, Self::Mutation)
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Observational => "observational",
            Self::Mutation => "mutation",
        }
    }

    const fn from_access(access: WorkspaceAccess) -> Self {
        match access {
            WorkspaceAccess::ReadOnly => Self::Observational,
            WorkspaceAccess::Writable => Self::Mutation,
        }
    }
}

enum ProductShellDispatcher<'gateway, 'authority> {
    Raw(RawShellDispatcher<'gateway, 'authority>),
    Native(ShellDispatcher<'gateway, 'authority, native_gate::GateBackend>),
}

impl ProductShellDispatcher<'_, '_> {
    fn process_control(&self) -> Option<peritus_process::ProcessControl> {
        match self {
            Self::Raw(dispatcher) => dispatcher.process_control(),
            Self::Native(dispatcher) => dispatcher.process_control(),
        }
    }
}

impl ToolDispatcher for ProductShellDispatcher<'_, '_> {
    fn implementation_identity(&self) -> &ImplementationIdentity {
        match self {
            Self::Raw(dispatcher) => dispatcher.implementation_identity(),
            Self::Native(dispatcher) => dispatcher.implementation_identity(),
        }
    }

    fn descriptor_digest(&self) -> SchemaDigest {
        match self {
            Self::Raw(dispatcher) => dispatcher.descriptor_digest(),
            Self::Native(dispatcher) => dispatcher.descriptor_digest(),
        }
    }

    fn start(&mut self, invocation: AuthorizedInvocation) -> Result<ToolStart, DispatchFailure> {
        match self {
            Self::Raw(dispatcher) => dispatcher.start(invocation),
            Self::Native(dispatcher) => dispatcher.start(invocation),
        }
    }
}

#[derive(Clone)]
pub(super) struct ProgressBatch {
    events: Vec<ToolProgress>,
    page: Option<ProgressPage>,
}

impl ProgressBatch {
    const fn empty() -> Self {
        Self { events: Vec::new(), page: None }
    }
}

enum ObservationTarget {
    Terminal(TerminalCommand),
    Recovered(Value),
    Starting {
        process_id: peritus_types::ProcessId,
        interactive: bool,
        elapsed_millis: u64,
    },
    Active {
        router: CommandRouter,
        control: Option<peritus_process::ProcessControl>,
        invocation: InvocationHandle,
        observed_at: AuthorityInstant,
    },
}

enum ObservedCommand {
    Active(ProgressBatch),
    SettlementPending(DispatchFailure, ProgressBatch),
    Terminal(ToolResult, ProgressBatch),
    Indeterminate(String),
}

struct AdvancedObservation {
    observation: AdvancedObservationKind,
    retain_projection: bool,
    control: Option<peritus_process::ProcessControl>,
    control_admitted: Option<bool>,
}

enum AdvancedObservationKind {
    Terminal(TerminalCommand),
    Recovered(Value),
    Starting {
        process_id: peritus_types::ProcessId,
        interactive: bool,
        elapsed_millis: u64,
    },
    Active(ProgressBatch),
    OperationFailed(RouterError),
    SettlementPending(DispatchFailure, ProgressBatch),
}

struct StartedCommand {
    handle: String,
    process_id: peritus_types::ProcessId,
    control: Option<peritus_process::ProcessControl>,
    projection: Value,
}

struct CancelOnDrop {
    cancellation: CancellationToken,
    armed: bool,
}

struct ReconcileObservationOnDrop {
    runtime: CommandRuntime,
    handle: String,
    control: Option<peritus_process::ProcessControl>,
    armed: bool,
}

struct StartedDelivery {
    runtime: CommandRuntime,
    handle: String,
    control: Option<peritus_process::ProcessControl>,
    value: Option<Value>,
    armed: bool,
}

impl StartedDelivery {
    fn new(runtime: CommandRuntime, started: StartedCommand) -> Self {
        let StartedCommand { handle, control, projection, .. } = started;
        let value = projection;
        Self { runtime, handle, control, value: Some(value), armed: true }
    }

    fn deliver(mut self) -> Value {
        self.armed = false;
        self.value.take().expect("started command delivery retains its value")
    }
}

impl Drop for StartedDelivery {
    fn drop(&mut self) {
        if self.armed {
            self.runtime.inner.cancellation_worker.enqueue_observer_reconciliation(
                self.runtime.clone(),
                self.handle.clone(),
                self.control.clone(),
            );
        }
    }
}

impl CancelOnDrop {
    fn new(cancellation: CancellationToken) -> Self {
        Self { cancellation, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        if self.armed {
            let _ = self.cancellation.cancel();
        }
    }
}

impl ReconcileObservationOnDrop {
    fn new(
        runtime: CommandRuntime,
        handle: String,
        control: Option<peritus_process::ProcessControl>,
    ) -> Self {
        Self { runtime, handle, control, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for ReconcileObservationOnDrop {
    fn drop(&mut self) {
        if self.armed {
            self.runtime.inner.cancellation_worker.enqueue_observer_reconciliation(
                self.runtime.clone(),
                self.handle.clone(),
                self.control.clone(),
            );
        }
    }
}

/// Fully checked input for one command start.
pub(super) struct StartCommand<'a> {
    pub(super) program: &'a str,
    pub(super) arguments: &'a [String],
    pub(super) cwd: &'a Path,
    pub(super) timeout: Option<Duration>,
    pub(super) interactive: bool,
    pub(super) rows: u16,
    pub(super) columns: u16,
    pub(super) idempotency_key: &'a str,
    pub(super) environment: Vec<(String, String)>,
    pub(super) protected_paths: &'a [PathBuf],
}

struct OwnedStartCommand {
    program: String,
    arguments: Vec<String>,
    cwd: PathBuf,
    timeout: Option<Duration>,
    interactive: bool,
    rows: u16,
    columns: u16,
    idempotency_key: String,
    environment: Vec<(String, String)>,
    protected_paths: Vec<PathBuf>,
}

impl OwnedStartCommand {
    fn from_borrowed(request: StartCommand<'_>) -> Self {
        Self {
            program: request.program.to_owned(),
            arguments: request.arguments.to_vec(),
            cwd: request.cwd.to_path_buf(),
            timeout: request.timeout,
            interactive: request.interactive,
            rows: request.rows,
            columns: request.columns,
            idempotency_key: request.idempotency_key.to_owned(),
            environment: request.environment,
            protected_paths: request.protected_paths.to_vec(),
        }
    }

    fn borrowed(&self) -> StartCommand<'_> {
        StartCommand {
            program: &self.program,
            arguments: &self.arguments,
            cwd: &self.cwd,
            timeout: self.timeout,
            interactive: self.interactive,
            rows: self.rows,
            columns: self.columns,
            idempotency_key: &self.idempotency_key,
            environment: self.environment.clone(),
            protected_paths: &self.protected_paths,
        }
    }
}

impl CommandRuntime {
    /// Selects the run's local-memory policy without changing command authority or recovery.
    ///
    /// # Errors
    /// Rejects malformed local-memory bounds or subprocess configuration.
    pub fn with_local_context(
        mut self,
        config: crate::LocalContextConfig,
    ) -> Result<Self, crate::ProductRunnerError> {
        config.validate().map_err(|error| runtime_open(error.to_string()))?;
        self.local_context = config;
        Ok(self)
    }

    pub(crate) const fn local_context_config(&self) -> &crate::LocalContextConfig {
        &self.local_context
    }

    /// Selects the trusted host catalog used by exact-target managed-network gates.
    #[must_use]
    pub fn with_managed_gate_network(
        mut self,
        workspace_id: peritus_types::WorkspaceId,
        catalog: ManagedGateNetworkCatalog,
    ) -> Self {
        self.managed_gate_workspace = Some(workspace_id);
        self.managed_gate_network = catalog;
        self
    }

    pub(crate) fn managed_gate_network(
        &self,
    ) -> Option<(peritus_types::WorkspaceId, &ManagedGateNetworkCatalog)> {
        self.managed_gate_workspace.map(|workspace| (workspace, &self.managed_gate_network))
    }

    #[cfg(test)]
    pub(crate) fn open_for_test(workspace_root: &Path, run_id: RunId) -> Self {
        let state_guard = tempfile::tempdir().expect("temporary command state");
        let processes = peritus_process::ProcessStore::open(
            state_guard.path().join("processes"),
            workspace_root,
        )
        .expect("test command process store");
        let mut runtime =
            Self::open(state_guard.path().join("router"), workspace_root, run_id, processes)
                .expect("test command runtime");
        Arc::get_mut(&mut runtime.inner).expect("new command runtime is unique").state_guard =
            Some(state_guard);
        runtime
    }

    pub(super) fn start(&self, request: StartCommand<'_>) -> Result<Value, DeveloperLoopError> {
        self.start_in_mode(request, CommandExecutionMode::Mutation, None)
    }

    pub(super) fn start_observational(
        &self,
        request: StartCommand<'_>,
    ) -> Result<Value, DeveloperLoopError> {
        self.start_in_mode(request, CommandExecutionMode::Observational, None)
    }

    pub(super) fn start_selected(
        &self,
        request: StartCommand<'_>,
        mode: CommandExecutionMode,
        resource_evidence: Value,
    ) -> Result<Value, DeveloperLoopError> {
        self.start_in_mode(request, mode, Some(resource_evidence))
    }

    fn start_in_mode(
        &self,
        request: StartCommand<'_>,
        mode: CommandExecutionMode,
        resource_evidence: Option<Value>,
    ) -> Result<Value, DeveloperLoopError> {
        let cancellation = CancellationToken::new();
        let started = self.start_owned(request, mode, resource_evidence, &cancellation)?;
        Ok(started.projection)
    }

    pub(super) async fn start_async(
        &self,
        request: StartCommand<'_>,
    ) -> Result<Value, DeveloperLoopError> {
        self.start_async_in_mode(request, CommandExecutionMode::Mutation, None).await
    }

    pub(super) async fn start_observational_async(
        &self,
        request: StartCommand<'_>,
    ) -> Result<Value, DeveloperLoopError> {
        self.start_async_in_mode(request, CommandExecutionMode::Observational, None).await
    }

    pub(super) async fn start_selected_async(
        &self,
        request: StartCommand<'_>,
        mode: CommandExecutionMode,
        resource_evidence: Value,
    ) -> Result<Value, DeveloperLoopError> {
        self.start_async_in_mode(request, mode, Some(resource_evidence)).await
    }

    async fn start_async_in_mode(
        &self,
        request: StartCommand<'_>,
        mode: CommandExecutionMode,
        resource_evidence: Option<Value>,
    ) -> Result<Value, DeveloperLoopError> {
        let request = OwnedStartCommand::from_borrowed(request);
        let cancellation = CancellationToken::new();
        let mut cancel_on_drop = CancelOnDrop::new(cancellation.clone());
        let runtime = self.clone();
        let worker = tokio::task::spawn_blocking(move || {
            let started =
                runtime.start_owned(request.borrowed(), mode, resource_evidence, &cancellation)?;
            if cancellation.is_cancelled() {
                runtime.inner.cancellation_worker.enqueue_observer_reconciliation(
                    runtime.clone(),
                    started.handle.clone(),
                    started.control.clone(),
                );
                return Err(DeveloperLoopError::Cancelled);
            }
            Ok(StartedDelivery::new(runtime, started))
        });
        let joined = worker.await;
        cancel_on_drop.disarm();
        let delivery =
            joined.map_err(|error| tool(format!("command admission worker failed: {error}")))??;
        Ok(delivery.deliver())
    }

    pub(super) fn run(&self, request: StartCommand<'_>) -> Result<Value, DeveloperLoopError> {
        self.run_in_mode(request, CommandExecutionMode::Mutation, None)
    }

    pub(super) fn run_observational(
        &self,
        request: StartCommand<'_>,
    ) -> Result<Value, DeveloperLoopError> {
        self.run_in_mode(request, CommandExecutionMode::Observational, None)
    }

    pub(super) fn run_selected(
        &self,
        request: StartCommand<'_>,
        mode: CommandExecutionMode,
        resource_evidence: Value,
    ) -> Result<Value, DeveloperLoopError> {
        self.run_in_mode(request, mode, Some(resource_evidence))
    }

    fn run_in_mode(
        &self,
        request: StartCommand<'_>,
        mode: CommandExecutionMode,
        resource_evidence: Option<Value>,
    ) -> Result<Value, DeveloperLoopError> {
        let cancellation = CancellationToken::new();
        if tokio::runtime::Handle::try_current().is_ok_and(|handle| {
            matches!(handle.runtime_flavor(), tokio::runtime::RuntimeFlavor::MultiThread)
        }) {
            // Tool execution is a synchronous interface. Tell Tokio before waiting so this run's
            // worker can be replaced and the daemon control plane remains schedulable.
            return tokio::task::block_in_place(|| {
                self.run_to_completion(request, mode, resource_evidence, &cancellation)
            });
        }
        self.run_to_completion(request, mode, resource_evidence, &cancellation)
    }

    pub(super) async fn run_async(
        &self,
        request: StartCommand<'_>,
    ) -> Result<Value, DeveloperLoopError> {
        self.run_async_in_mode(request, CommandExecutionMode::Mutation, None).await
    }

    pub(super) async fn run_observational_async(
        &self,
        request: StartCommand<'_>,
    ) -> Result<Value, DeveloperLoopError> {
        self.run_async_in_mode(request, CommandExecutionMode::Observational, None).await
    }

    pub(super) async fn run_selected_async(
        &self,
        request: StartCommand<'_>,
        mode: CommandExecutionMode,
        resource_evidence: Value,
    ) -> Result<Value, DeveloperLoopError> {
        self.run_async_in_mode(request, mode, Some(resource_evidence)).await
    }

    async fn run_async_in_mode(
        &self,
        request: StartCommand<'_>,
        mode: CommandExecutionMode,
        resource_evidence: Option<Value>,
    ) -> Result<Value, DeveloperLoopError> {
        let request = OwnedStartCommand::from_borrowed(request);
        let cancellation = CancellationToken::new();
        let mut cancel_on_drop = CancelOnDrop::new(cancellation.clone());
        let runtime = self.clone();
        let worker = tokio::task::spawn_blocking(move || {
            runtime.run_to_completion(
                request.borrowed(),
                mode,
                resource_evidence,
                &cancellation,
            )
        });
        let joined = worker.await;
        cancel_on_drop.disarm();
        joined.map_err(|error| tool(format!("command execution worker failed: {error}")))?
    }

    fn run_to_completion(
        &self,
        request: StartCommand<'_>,
        mode: CommandExecutionMode,
        resource_evidence: Option<Value>,
        cancellation: &CancellationToken,
    ) -> Result<Value, DeveloperLoopError> {
        let started = self.start_owned(request, mode, resource_evidence, cancellation)?;
        if started.projection.get("state").and_then(Value::as_str) != Some("running") {
            return Ok(started.projection);
        }
        loop {
            if cancellation.is_cancelled() {
                self.inner.cancellation_worker.enqueue_observer_reconciliation(
                    self.clone(),
                    started.handle.clone(),
                    started.control.clone(),
                );
                return Err(DeveloperLoopError::Cancelled);
            }
            let observation = self.poll(&started.handle);
            if cancellation.is_cancelled() {
                self.inner.cancellation_worker.enqueue_observer_reconciliation(
                    self.clone(),
                    started.handle.clone(),
                    started.control.clone(),
                );
                return Err(DeveloperLoopError::Cancelled);
            }
            let observation = observation?;
            if observation
                .get("settlement_pending")
                .and_then(Value::as_bool)
                == Some(true)
                || observation.get("operation_failed").and_then(Value::as_bool) == Some(true)
            {
                return Ok(observation);
            }
            if observation.get("state").and_then(Value::as_str) != Some("running") {
                return Ok(observation);
            }
            thread::sleep(POLL_INTERVAL);
        }
    }

    pub(super) fn poll(&self, handle: &str) -> Result<Value, DeveloperLoopError> {
        self.observe(handle, Observation::Poll)
    }

    pub(super) async fn poll_async(&self, handle: String) -> Result<Value, DeveloperLoopError> {
        self.observe_async(handle, Observation::Poll).await
    }

    pub(super) fn output(
        &self,
        handle: &str,
        label: &str,
        digest: &str,
        prepared_digest: &str,
        size: u64,
        offset: u64,
    ) -> Result<Value, DeveloperLoopError> {
        let projection = {
            let state = self.inner.state.lock().map_err(|_| tool("command runtime is poisoned"))?;
            if let Some(terminal) = state.terminal.get(handle) {
                result::terminal_deferred(handle, &terminal.result, &terminal.progress)
            } else if let Some(recovered) = state.recovered.get(handle) {
                recovered.clone()
            } else if state.starting.contains_key(handle) || state.active.contains_key(handle) {
                return Err(tool("command output is available only after terminal completion"));
            } else {
                return Err(tool("command invocation handle is unknown"));
            }
        };
        result::read_output_page(
            &projection,
            &self.inner.artifacts,
            handle,
            label,
            digest,
            prepared_digest,
            size,
            offset,
        )
        .map_err(tool)
    }

    fn start_owned(
        &self,
        request: StartCommand<'_>,
        mode: CommandExecutionMode,
        resource_evidence: Option<Value>,
        cancellation: &CancellationToken,
    ) -> Result<StartedCommand, DeveloperLoopError> {
        ensure_not_cancelled(cancellation)?;
        let cwd = canonical_command_cwd(&self.inner.workspace_root, request.cwd)?;
        let timeout_millis = request
            .timeout
            .map(|timeout| {
                u64::try_from(timeout.as_millis())
                    .map_err(|_| tool("command timeout is not representable in milliseconds"))
            })
            .transpose()?;
        let after = self
            .inner
            .state
            .lock()
            .map_err(|_| tool("command runtime is poisoned"))?
            .next_ordinal;
        // Durable allocation may wait behind another process for an unbounded amount of time.
        // Do not hold the active-command owner while queued: existing command poll/control/cancel
        // operations must remain responsive independently of admission for new work.
        let ordinal = match ordinal::reserve_cancellable(
            &self.inner.state_root,
            self.inner.run_id,
            after,
            cancellation,
        ) {
            Ok(ordinal) => ordinal,
            Err(ordinal::ReserveError::Cancelled) => return Err(DeveloperLoopError::Cancelled),
            Err(ordinal::ReserveError::Storage(error)) => return Err(tool(error)),
        };
        ensure_not_cancelled(cancellation)?;
        let contract = contract::command_contract(self.inner.run_id, ordinal).map_err(tool)?;
        let ids = identity::CommandIds::new(self.inner.run_id, ordinal, &contract).map_err(tool)?;
        {
            let mut state = self
                .inner
                .state
                .lock()
                .map_err(|_| tool("command runtime is poisoned"))?;
            state.next_ordinal = state.next_ordinal.max(ordinal);
        }
        let command = plan::compile(
            &self.inner.registry,
            &ids,
            &self.inner.workspace_root,
            &self.inner.state_root,
            cancellation,
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
                mode,
                protected_paths: request.protected_paths,
            },
        )
        .map_err(tool)?;
        ensure_not_cancelled(cancellation)?;
        let authority_root =
            self.inner.state_root.join("authority").join(identity::action_hex(ids.action));
        let tool_authority = authority::commit_tool(
            &authority_root.join("c4.sqlite3"),
            &ids,
            &contract,
            &command.prepared,
            timeout_millis.unwrap_or(0),
        )
        .map_err(tool)?;
        ensure_not_cancelled(cancellation)?;
        let process_authority = authority::commit_process(
            &authority_root.join("c2.sqlite3"),
            &ids,
            &contract,
            &command.execution,
            timeout_millis.unwrap_or(0),
        )
        .map_err(tool)?;
        ensure_not_cancelled(cancellation)?;
        let process_request = process_authority.request(&ids, &command.execution);
        let artifacts = ArtifactStore::open(self.inner.artifacts.clone())
            .map_err(|error| tool(error.to_string()))?;
        let mut dispatcher = match command.backend {
            plan::CommandBackend::Raw { .. } => ProductShellDispatcher::Raw(
                RawShellDispatcher::new(
                    &self.inner.gateway,
                    &process_request,
                    command.execution.clone(),
                    artifacts,
                )
                .map_err(|error| tool(error.to_string()))?,
            ),
            plan::CommandBackend::Native(prepared) => {
                ProductShellDispatcher::Native(
                    ShellDispatcher::new(
                        &self.inner.gateway,
                        &process_request,
                        command.execution.clone(),
                        prepared.checked,
                        prepared.admission,
                        prepared.backend,
                        artifacts,
                    )
                    .map_err(|error| tool(error.to_string()))?,
                )
            }
        };
        let tool_request = tool_authority.request(&ids, &command.prepared);
        ensure_not_cancelled(cancellation)?;
        let reservation = match self.inner.replay_index.reserve_cancellable(
            ids.action,
            command.prepared.replay_identity().digest(),
            cancellation,
        ) {
            Ok(reservation) => reservation,
            Err(replay_index::OperationError::Cancelled) => {
                return Err(DeveloperLoopError::Cancelled);
            }
            Err(replay_index::OperationError::Store(error)) => {
                return Err(tool(error.to_string()));
            }
        };
        ensure_not_cancelled(cancellation)?;
        let router_owner = Arc::new(Mutex::new(ToolRouter::with_replay_store(
            self.inner.registry.clone(),
            self.inner.router_limits,
            self.inner.replay_index.clone(),
        )));
        let handle = identity::action_hex(ids.action);
        let started = Instant::now();
        {
            let mut state = self
                .inner
                .state
                .lock()
                .map_err(|_| tool("command runtime is poisoned"))?;
            match state.starting.entry(handle.clone()) {
                Entry::Vacant(slot) => {
                    slot.insert(StartingCommand {
                        plan: command.execution.clone(),
                        router: Arc::clone(&router_owner),
                        started,
                        interactive: request.interactive,
                        resource_evidence: resource_evidence.clone(),
                        protected_paths: request.protected_paths.to_vec(),
                    });
                }
                Entry::Occupied(_) => {
                    return Err(tool("command start reused a retained invocation identity"));
                }
            }
        }
        let mut router = router_owner
            .lock()
            .map_err(|_| tool("command router is poisoned"))?;
        let dispatch = catch_unwind(AssertUnwindSafe(|| {
            router.dispatch_reserved(
                &command.prepared,
                &tool_request,
                &mut dispatcher,
                reservation,
            )
        }));
        let outcome = match dispatch {
            Ok(Ok(outcome)) => outcome,
            Ok(Err(failure)) => {
                let (error, returned) = failure.into_parts();
                if returned.is_some() {
                    let mut state = self
                        .inner
                        .state
                        .lock()
                        .map_err(|_| tool("command runtime is poisoned"))?;
                    if starting_owner_matches(&state, &handle, &router_owner) {
                        state.starting.remove(&handle);
                    }
                    return Err(tool(error.to_string()));
                }
                let projection = with_execution_context(
                    result::indeterminate(
                        &handle,
                        &format!(
                            "command dispatch failed after authority consumption; the effect was not dispatched again: {error}"
                        ),
                    ),
                    Some(mode),
                    resource_evidence.as_ref(),
                );
                let publication_pending =
                    router.pending_replay_publication(ids.action).is_some();
                let mut state = self
                    .inner
                    .state
                    .lock()
                    .map_err(|_| tool("command runtime is poisoned"))?;
                if !starting_owner_matches(&state, &handle, &router_owner) {
                    return Err(tool(
                        "command starting ownership changed during dispatch failure settlement",
                    ));
                }
                state.starting.remove(&handle);
                state.recovered.insert(handle.clone(), projection.clone());
                if publication_pending {
                    state.replay_pending.insert(ids.action, Arc::clone(&router_owner));
                }
                drop(state);
                drop(router);
                self.retain_projection(&handle, projection.clone());
                if publication_pending {
                    self.publish_replay_action(ids.action);
                }
                return Ok(StartedCommand {
                    handle,
                    process_id: ids.process,
                    control: None,
                    projection,
                });
            }
            Err(_) => {
                let reconciled = catch_unwind(AssertUnwindSafe(|| {
                    router.reconcile_interrupted_dispatch(&command.prepared)
                }));
                let reconciled = match reconciled {
                    Ok(result) => result.map_err(|error| {
                        tool(format!(
                            "command dispatch was interrupted; retained reconciliation handle {handle}: {error}"
                        ))
                    })?,
                    Err(_)
                        if router.pending_replay_publication(ids.action).is_some() =>
                    {
                        InterruptedDispatch::Settled
                    }
                    Err(_) => {
                        drop(router);
                        return Err(tool(format!(
                            "command dispatch was interrupted; reconciliation remains pending for handle {handle}"
                        )));
                    }
                };
                match reconciled {
                    InterruptedDispatch::Unadopted => {
                        let mut state = self
                            .inner
                            .state
                            .lock()
                            .map_err(|_| tool("command runtime is poisoned"))?;
                        if starting_owner_matches(&state, &handle, &router_owner) {
                            state.starting.remove(&handle);
                        }
                        return Err(tool(
                            "command dispatch was interrupted before invocation admission",
                        ));
                    }
                    InterruptedDispatch::Active(invocation) => {
                        let control = dispatcher.process_control();
                        let mut state = self
                            .inner
                            .state
                            .lock()
                            .map_err(|_| tool("command runtime is poisoned"))?;
                        if !starting_owner_matches(&state, &handle, &router_owner) {
                            return Err(tool(
                                "command starting ownership changed during dispatch reconciliation",
                            ));
                        }
                        state.starting.remove(&handle);
                        state.active.insert(
                            handle.clone(),
                            ActiveCommand {
                                plan: command.execution,
                                control: control.clone(),
                                router: Arc::clone(&router_owner),
                                invocation,
                                started,
                                interactive: request.interactive,
                                resource_evidence: resource_evidence.clone(),
                                protected_paths: request.protected_paths.to_vec(),
                            },
                        );
                        drop(state);
                        drop(router);
                        let projection = with_execution_context(
                            result::active(&handle, &ProgressBatch::empty()),
                            Some(mode),
                            resource_evidence.as_ref(),
                        );
                        self.retain_projection(&handle, projection.clone());
                        return Ok(StartedCommand {
                            handle,
                            process_id: ids.process,
                            control,
                            projection,
                        });
                    }
                    InterruptedDispatch::Settled => {
                        let projection = with_execution_context(
                            result::indeterminate(
                                &handle,
                                "command dispatch was interrupted after authority consumption; the effect was not dispatched again",
                            ),
                            Some(mode),
                            resource_evidence.as_ref(),
                        );
                        let publication_pending =
                            router.pending_replay_publication(ids.action).is_some();
                        let mut state = self
                            .inner
                            .state
                            .lock()
                            .map_err(|_| tool("command runtime is poisoned"))?;
                        if !starting_owner_matches(&state, &handle, &router_owner) {
                            return Err(tool(
                                "command starting ownership changed during dispatch settlement",
                            ));
                        }
                        state.starting.remove(&handle);
                        state.recovered.insert(handle.clone(), projection.clone());
                        if publication_pending {
                            state.replay_pending.insert(ids.action, Arc::clone(&router_owner));
                        }
                        drop(state);
                        drop(router);
                        self.retain_projection(&handle, projection.clone());
                        if publication_pending {
                            self.publish_replay_action(ids.action);
                        }
                        return Ok(StartedCommand {
                            handle,
                            process_id: ids.process,
                            control: None,
                            projection,
                        });
                    }
                }
            }
        };
        let mut started_control = None;
        let mut publication_pending = false;
        let mut state = self
            .inner
            .state
            .lock()
            .map_err(|_| tool("command runtime is poisoned"))?;
        if !starting_owner_matches(&state, &handle, &router_owner) {
            return Err(tool("command starting ownership changed before dispatch settlement"));
        }
        state.starting.remove(&handle);
        let terminal_projection = match outcome {
            DispatchOutcome::Active(invocation) => {
                let control = dispatcher.process_control();
                state.active.insert(
                    handle.clone(),
                    ActiveCommand {
                        plan: command.execution,
                        control: control.clone(),
                        router: Arc::clone(&router_owner),
                        invocation,
                        started,
                        interactive: request.interactive,
                        resource_evidence: resource_evidence.clone(),
                        protected_paths: request.protected_paths.to_vec(),
                    },
                );
                started_control = control;
                None
            }
            DispatchOutcome::Completed(terminal) | DispatchOutcome::Replayed(terminal) => {
                let retained = terminal.clone();
                state.terminal.insert(
                    handle.clone(),
                    TerminalCommand {
                        result: terminal,
                        progress: ProgressBatch::empty(),
                        mode,
                        resource_evidence: resource_evidence.clone(),
                    },
                );
                publication_pending = router.pending_replay_publication(ids.action).is_some();
                if publication_pending {
                    state.replay_pending.insert(ids.action, Arc::clone(&router_owner));
                }
                Some(retained)
            }
            DispatchOutcome::PriorOutcome(disposition) => {
                return Err(tool(format!(
                    "command has prior non-replayable outcome: {disposition:?}"
                )));
            }
        };
        drop(state);
        drop(router);
        let terminal_action = terminal_projection.as_ref().map(ToolResult::action_id);
        let projection = with_execution_context(
            match terminal_projection {
                Some(terminal) => match result::terminal(
                    &handle,
                    &terminal,
                    &self.inner.artifacts,
                    &ProgressBatch::empty(),
                ) {
                    Ok(projection) => projection,
                    Err(error) => {
                        self.enqueue_observer_reconciliation(&handle, started_control.clone());
                        return Err(tool(error));
                    }
                },
                None => result::active(&handle, &ProgressBatch::empty()),
            },
            Some(mode),
            resource_evidence.as_ref(),
        );
        if terminal_action.is_some() {
            if let Err(error) = self.record_projection(&handle, projection.clone()) {
                self.enqueue_observer_reconciliation(&handle, started_control.clone());
                return Err(error);
            }
        } else {
            self.retain_projection(&handle, projection.clone());
        }
        if publication_pending {
            self.publish_replay_action(ids.action);
            if matches!(self.observer_reconciliation_settled(&handle), Ok(false)) {
                self.enqueue_observer_reconciliation(&handle, started_control.clone());
            }
        }
        Ok(StartedCommand {
            handle,
            process_id: ids.process,
            control: started_control,
            projection,
        })
    }

    fn publish_replay_action(&self, action_id: ActionId) {
        let cancellation = CancellationToken::new();
        let owner = {
            let state = match self.inner.state.lock() {
                Ok(state) => state,
                Err(_) => {
                    crate::diagnostic::report(
                        "peritus command runtime: replay publication backlog is poisoned",
                    );
                    return;
                }
            };
            state.replay_pending.get(&action_id).cloned()
        };
        let Some(owner) = owner else { return };
        let pending = match owner.lock() {
            Ok(router) => router.pending_replay_publication(action_id),
            Err(_) => {
                crate::diagnostic::report(
                    "peritus command runtime: pending replay command router is poisoned",
                );
                return;
            }
        };
        let Some(pending) = pending else {
            crate::diagnostic::report(
                "peritus command runtime: retained replay publication owner has no settled receipt",
            );
            return;
        };
        let published = match self
            .inner
            .replay_index
            .publish_cancellable(&pending, &cancellation)
        {
            Ok(published) => published,
            Err(replay_index::OperationError::Cancelled) => {
                crate::diagnostic::report(
                    "peritus command runtime: replay receipt publication was cancelled",
                );
                return;
            }
            Err(replay_index::OperationError::Store(error)) => {
                crate::diagnostic::report(&format!(
                    "peritus command runtime: retain pending replay receipt: {error}"
                ));
                return;
            }
        };
        let acknowledgement = match owner.lock() {
            Ok(mut router) => router.acknowledge_replay_publication(published),
            Err(_) => {
                crate::diagnostic::report(
                    "peritus command runtime: acknowledge replay publication: command router is poisoned",
                );
                return;
            }
        };
        if let Err(error) = acknowledgement {
            crate::diagnostic::report(&format!(
                "peritus command runtime: acknowledge replay publication: {error}"
            ));
            return;
        }
        self.clear_replay_owner(action_id, &owner);
    }

    fn clear_replay_owner(&self, action_id: ActionId, owner: &CommandRouter) {
        let mut state = match self.inner.state.lock() {
            Ok(state) => state,
            Err(_) => {
                crate::diagnostic::report(
                    "peritus command runtime: retire replay publication owner: backlog is poisoned",
                );
                return;
            }
        };
        if state
            .replay_pending
            .get(&action_id)
            .is_some_and(|retained| Arc::ptr_eq(retained, owner))
        {
            state.replay_pending.remove(&action_id);
        }
    }

    async fn observe_async(
        &self,
        handle: String,
        operation: Observation,
    ) -> Result<Value, DeveloperLoopError> {
        let mut reconcile_on_drop = self.reconcile_observation_on_drop(&handle)?;
        let runtime = self.clone();
        let worker = tokio::task::spawn_blocking(move || runtime.observe(&handle, operation));
        match worker.await {
            Ok(observation) => {
                if let Some(reconcile_on_drop) = &mut reconcile_on_drop {
                    reconcile_on_drop.disarm();
                }
                observation
            }
            Err(error) => Err(tool(format!("command observation worker failed: {error}"))),
        }
    }

    fn reconcile_observation_on_drop(
        &self,
        handle: &str,
    ) -> Result<Option<ReconcileObservationOnDrop>, DeveloperLoopError> {
        let state = self.inner.state.lock().map_err(|_| tool("command runtime is poisoned"))?;
        Ok(state.active.get(handle).map(|active| {
            ReconcileObservationOnDrop::new(
                self.clone(),
                handle.to_owned(),
                active.control.clone(),
            )
        }))
    }

    fn observe(&self, handle: &str, operation: Observation) -> Result<Value, DeveloperLoopError> {
        let context = self.projection_context(handle)?;
        let advanced = match self.advance_observation(handle, operation) {
            Ok(advanced) => advanced,
            Err(error) => {
                if matches!(self.observer_reconciliation_settled(handle), Ok(false)) {
                    self.enqueue_observer_reconciliation(handle, None);
                }
                return Err(error);
            }
        };
        let AdvancedObservation {
            observation,
            retain_projection: persist_projection,
            control,
            control_admitted,
        } = advanced;
        let publication_action = match &observation {
            AdvancedObservationKind::Terminal(terminal) => Some(terminal.result.action_id()),
            AdvancedObservationKind::Recovered(_) => {
                self.pending_replay_action_for_handle(handle)?
            }
            AdvancedObservationKind::Starting { .. }
            | AdvancedObservationKind::Active(_)
            | AdvancedObservationKind::OperationFailed(_)
            | AdvancedObservationKind::SettlementPending(_, _) => None,
        };
        let settlement_pending =
            matches!(&observation, AdvancedObservationKind::SettlementPending(_, _));
        let reconciliation_required = settlement_pending
            || matches!(&observation, AdvancedObservationKind::OperationFailed(_))
            || publication_action.is_some();
        let value = match self.render_advanced_observation(handle, observation) {
            Ok(value) => value,
            Err(error) => {
                if reconciliation_required {
                    self.enqueue_observer_reconciliation(handle, control);
                }
                return Err(error);
            }
        };
        let mut value = with_execution_context(
            value,
            context.mode,
            context.resource_evidence.as_ref(),
        );
        if let Some(admitted) = control_admitted {
            value
                .as_object_mut()
                .ok_or_else(|| tool("command control observation is not an object"))?
                .insert("control_admitted".to_owned(), Value::Bool(admitted));
        }
        if persist_projection {
            if publication_action.is_some() {
                if let Err(error) = self.record_projection(handle, value.clone()) {
                    self.enqueue_observer_reconciliation(handle, control);
                    return Err(error);
                }
            } else {
                self.retain_projection(handle, value.clone());
            }
        }
        if let Some(action_id) = publication_action {
            self.publish_replay_action(action_id);
        }
        if reconciliation_required
            && !self.observer_reconciliation_settled(handle).unwrap_or(false)
        {
            self.enqueue_observer_reconciliation(handle, control);
        }
        Ok(value)
    }

    fn advance_observation(
        &self,
        handle: &str,
        operation: Observation,
    ) -> Result<AdvancedObservation, DeveloperLoopError> {
        let control_requested = matches!(&operation, Observation::Control(_));
        let target = self.observation_target(handle)?;
        let (router_owner, control, invocation) = match target {
            ObservationTarget::Active { router, control, invocation, .. } => {
                (router, control, invocation)
            }
            ObservationTarget::Terminal(terminal) => {
                return Ok(AdvancedObservation {
                    observation: AdvancedObservationKind::Terminal(terminal),
                    retain_projection: true,
                    control: None,
                    control_admitted: control_requested.then_some(false),
                });
            }
            ObservationTarget::Recovered(value) => {
                let retain_projection = result::preview_pending(&value)
                    || self.pending_replay_action_for_handle(handle)?.is_some();
                return Ok(AdvancedObservation {
                    observation: AdvancedObservationKind::Recovered(value),
                    retain_projection,
                    control: None,
                    control_admitted: control_requested.then_some(false),
                });
            }
            ObservationTarget::Starting { process_id, interactive, elapsed_millis } => {
                return Ok(AdvancedObservation {
                    observation: AdvancedObservationKind::Starting {
                        process_id,
                        interactive,
                        elapsed_millis,
                    },
                    retain_projection: false,
                    control: None,
                    control_admitted: control_requested.then_some(false),
                });
            }
        };

        let mut operation = operation;
        if matches!(&operation, Observation::Cancel) {
            if let Some(control) = &control {
                if control.terminal_result().is_none() {
                    control
                        .cancel(peritus_process::CancellationReason::User)
                        .map_err(|error| tool(error.to_string()))?;
                }
                operation = Observation::Poll;
            }
        }

        // This mutex belongs only to the exact invocation. Keep it through RuntimeState settlement
        // so a same-handle observer cannot see the router terminal before the runtime map commit.
        let mut router = router_owner.lock().map_err(|_| tool("command router is poisoned"))?;
        let current = self.observation_target(handle)?;
        let (current_owner, current_invocation, observed_at) = match current {
            ObservationTarget::Active { router, invocation, observed_at, .. } => {
                (router, invocation, observed_at)
            }
            other => {
                drop(router);
                let observation = match other {
                    ObservationTarget::Terminal(terminal) => {
                        AdvancedObservationKind::Terminal(terminal)
                    }
                    ObservationTarget::Recovered(value) => {
                        AdvancedObservationKind::Recovered(value)
                    }
                    ObservationTarget::Starting {
                        process_id,
                        interactive,
                        elapsed_millis,
                    } => AdvancedObservationKind::Starting {
                        process_id,
                        interactive,
                        elapsed_millis,
                    },
                    ObservationTarget::Active { .. } => {
                        return Err(tool("command invocation ownership changed during observation"));
                    }
                };
                let retain_projection = match &observation {
                    AdvancedObservationKind::Terminal(_) => true,
                    AdvancedObservationKind::Recovered(value) => {
                        result::preview_pending(value)
                            || self.pending_replay_action_for_handle(handle)?.is_some()
                    }
                    AdvancedObservationKind::Starting { .. }
                    | AdvancedObservationKind::Active(_)
                    | AdvancedObservationKind::OperationFailed(_)
                    | AdvancedObservationKind::SettlementPending(_, _) => false,
                };
                return Ok(AdvancedObservation {
                    observation,
                    retain_projection,
                    control: None,
                    control_admitted: control_requested.then_some(false),
                });
            }
        };
        if !Arc::ptr_eq(&router_owner, &current_owner) || invocation != current_invocation {
            return Err(tool("command invocation ownership changed during observation"));
        }

        let action_id = invocation.action_id();
        let observed = match operation {
            Observation::Recover => router.recover(invocation, observed_at).map(|outcome| {
                match outcome {
                    RecoveryOutcome::Active(update) => observed_update(update),
                    RecoveryOutcome::Completed(terminal) => {
                        ObservedCommand::Terminal(terminal, ProgressBatch::empty())
                    }
                    RecoveryOutcome::CompletedUpdate(update) => observed_update(update),
                    RecoveryOutcome::Indeterminate(failure) => ObservedCommand::Indeterminate(
                        failure.failure().detail().as_str().to_owned(),
                    ),
                }
            }),
            Observation::Poll => router.poll(invocation, observed_at).map(observed_update),
            Observation::Control(control) => {
                router.control(invocation, control, observed_at).map(observed_update)
            },
            Observation::Cancel => router
                .cancel(invocation, CancellationReason::Requested, observed_at)
                .map(observed_update),
        };
        let observed = match observed {
            Ok(observed) => observed,
            Err(error) => {
                let settled = !router.owns_active(invocation);
                if !settled {
                    // Failure of this request or observation never authorizes another process.
                    // Preserve both owners before returning the same-handle recovery receipt.
                    let state = self
                        .inner
                        .state
                        .lock()
                        .map_err(|_| tool("command runtime is poisoned"))?;
                    if !active_owner_matches(&state, handle, &router_owner, invocation)
                        || error.retained_invocation() != Some(invocation)
                    {
                        return Err(tool("command failure receipt differs from its retained owner"));
                    }
                    drop(state);
                    drop(router);
                    return Ok(AdvancedObservation {
                        observation: AdvancedObservationKind::OperationFailed(error),
                        // This receipt describes a failed operation, not a lifecycle transition.
                        // Keep the durable predecessor; a concurrent terminal writer must never
                        // be downgraded to running by a late rejected-control response.
                        retain_projection: false,
                        control,
                        control_admitted: control_requested.then_some(false),
                    });
                }
                let publication_pending = settled
                    && router.pending_replay_publication(action_id).is_some();
                let detail = error.to_string();
                let mut recovered = None;
                if settled {
                    let mut state = self
                        .inner
                        .state
                        .lock()
                        .map_err(|_| tool("command runtime is poisoned"))?;
                    if active_owner_matches(&state, handle, &router_owner, invocation) {
                        let active = state
                            .active
                            .get(handle)
                            .expect("matched active command retains its context");
                        let mode = CommandExecutionMode::from_access(
                            active.plan.working_directory().access(),
                        );
                        let resource_evidence = active.resource_evidence.clone();
                        state.active.remove(handle);
                        let projection = with_execution_context(
                            result::indeterminate(handle, &detail),
                            Some(mode),
                            resource_evidence.as_ref(),
                        );
                        state.recovered.insert(handle.to_owned(), projection.clone());
                        recovered = Some(projection);
                        if publication_pending {
                            state.replay_pending.insert(action_id, Arc::clone(&router_owner));
                        }
                    }
                }
                drop(router);
                if let Some(projection) = recovered {
                    if let Err(error) = self.record_projection(handle, projection) {
                        self.enqueue_observer_reconciliation(handle, None);
                        return Err(error);
                    }
                }
                if publication_pending {
                    self.publish_replay_action(action_id);
                    if matches!(self.observer_reconciliation_settled(handle), Ok(false)) {
                        self.enqueue_observer_reconciliation(handle, None);
                    }
                }
                return Err(tool(detail));
            }
        };

        let (active_mode, active_resource_evidence) = {
            let state = self.inner.state.lock().map_err(|_| tool("command runtime is poisoned"))?;
            let active = state
                .active
                .get(handle)
                .ok_or_else(|| tool("command invocation ownership changed before settlement"))?;
            (
                CommandExecutionMode::from_access(active.plan.working_directory().access()),
                active.resource_evidence.clone(),
            )
        };
        let indeterminate_projection = match &observed {
            ObservedCommand::Indeterminate(detail) => {
                Some(with_execution_context(
                    result::indeterminate(handle, detail),
                    Some(active_mode),
                    active_resource_evidence.as_ref(),
                ))
            }
            ObservedCommand::Active(_)
            | ObservedCommand::SettlementPending(_, _)
            | ObservedCommand::Terminal(_, _) => None,
        };
        let mut state = self.inner.state.lock().map_err(|_| tool("command runtime is poisoned"))?;
        if !active_owner_matches(&state, handle, &router_owner, invocation) {
            return Err(tool("command invocation ownership changed before settlement"));
        }
        let publication_pending = matches!(
            &observed,
            ObservedCommand::Terminal(_, _) | ObservedCommand::Indeterminate(_)
        )
            && router.pending_replay_publication(action_id).is_some();
        match &observed {
            ObservedCommand::Active(_) | ObservedCommand::SettlementPending(_, _) => {}
            ObservedCommand::Terminal(terminal, progress) => {
                state.active.remove(handle);
                state.terminal.insert(
                    handle.to_owned(),
                    TerminalCommand {
                        result: terminal.clone(),
                        progress: progress.clone(),
                        mode: active_mode,
                        resource_evidence: active_resource_evidence.clone(),
                    },
                );
                if publication_pending {
                    state.replay_pending.insert(action_id, Arc::clone(&router_owner));
                }
            }
            ObservedCommand::Indeterminate(_) => {
                state.active.remove(handle);
                state.recovered.insert(
                    handle.to_owned(),
                    indeterminate_projection
                        .as_ref()
                        .expect("indeterminate command retains its projection")
                        .clone(),
                );
                if publication_pending {
                    state.replay_pending.insert(action_id, Arc::clone(&router_owner));
                }
            }
        }
        drop(state);
        drop(router);

        let observation = match observed {
            ObservedCommand::Active(progress) => AdvancedObservationKind::Active(progress),
            ObservedCommand::SettlementPending(failure, progress) => {
                AdvancedObservationKind::SettlementPending(failure, progress)
            }
            ObservedCommand::Terminal(result, progress) => {
                AdvancedObservationKind::Terminal(TerminalCommand {
                    result,
                    progress,
                    mode: active_mode,
                    resource_evidence: active_resource_evidence,
                })
            }
            ObservedCommand::Indeterminate(_) => AdvancedObservationKind::Recovered(
                indeterminate_projection.expect("indeterminate command retains its projection"),
            ),
        };
        Ok(AdvancedObservation {
            observation,
            retain_projection: true,
            control,
            control_admitted: control_requested.then_some(true),
        })
    }

    fn observation_target(&self, handle: &str) -> Result<ObservationTarget, DeveloperLoopError> {
        let state = self.inner.state.lock().map_err(|_| tool("command runtime is poisoned"))?;
        if let Some(terminal) = state.terminal.get(handle) {
            return Ok(ObservationTarget::Terminal(terminal.clone()));
        }
        if let Some(recovered) = state.recovered.get(handle) {
            return Ok(ObservationTarget::Recovered(recovered.clone()));
        }
        if let Some(starting) = state.starting.get(handle) {
            let elapsed_millis = u64::try_from(starting.started.elapsed().as_millis())
                .unwrap_or(u64::MAX);
            return Ok(ObservationTarget::Starting {
                process_id: starting.plan.identity().process_id(),
                interactive: starting.interactive,
                elapsed_millis,
            });
        }
        let active = state
            .active
            .get(handle)
            .ok_or_else(|| tool("command invocation handle is unknown"))?;
        Ok(ObservationTarget::Active {
            router: Arc::clone(&active.router),
            control: active.control.clone(),
            invocation: active.invocation,
            observed_at: observed_at(active.started),
        })
    }

    pub(super) fn control_mode(
        &self,
        handle: &str,
    ) -> Result<Option<CommandExecutionMode>, DeveloperLoopError> {
        let state = self.inner.state.lock().map_err(|_| tool("command runtime is poisoned"))?;
        if let Some(command) = state.starting.get(handle) {
            return Ok(Some(CommandExecutionMode::from_access(
                command.plan.working_directory().access(),
            )));
        }
        if let Some(command) = state.active.get(handle) {
            return Ok(Some(CommandExecutionMode::from_access(
                command.plan.working_directory().access(),
            )));
        }
        if state.terminal.contains_key(handle) || state.recovered.contains_key(handle) {
            return Ok(None);
        }
        Err(tool("command invocation handle is unknown"))
    }

    pub(super) fn control_confinement(
        &self,
        handle: &str,
    ) -> Result<Option<Vec<PathBuf>>, DeveloperLoopError> {
        let state = self.inner.state.lock().map_err(|_| tool("command runtime is poisoned"))?;
        if let Some(command) = state.starting.get(handle) {
            return Ok(Some(command.protected_paths.clone()));
        }
        if let Some(command) = state.active.get(handle) {
            return Ok(Some(command.protected_paths.clone()));
        }
        if state.terminal.contains_key(handle) || state.recovered.contains_key(handle) {
            return Ok(None);
        }
        Err(tool("command invocation handle is unknown"))
    }

    fn projection_context(
        &self,
        handle: &str,
    ) -> Result<ProjectionContext, DeveloperLoopError> {
        let state = self.inner.state.lock().map_err(|_| tool("command runtime is poisoned"))?;
        if let Some(command) = state.starting.get(handle) {
            return Ok(ProjectionContext {
                mode: Some(CommandExecutionMode::from_access(
                    command.plan.working_directory().access(),
                )),
                resource_evidence: command.resource_evidence.clone(),
            });
        }
        if let Some(command) = state.active.get(handle) {
            return Ok(ProjectionContext {
                mode: Some(CommandExecutionMode::from_access(
                    command.plan.working_directory().access(),
                )),
                resource_evidence: command.resource_evidence.clone(),
            });
        }
        if let Some(command) = state.terminal.get(handle) {
            return Ok(ProjectionContext {
                mode: Some(command.mode),
                resource_evidence: command.resource_evidence.clone(),
            });
        }
        let recovered = state.recovered.get(handle);
        Ok(ProjectionContext {
            mode: recovered.and_then(execution_mode_from_value),
            resource_evidence: recovered
                .and_then(|value| value.get("execution_resources"))
                .cloned(),
        })
    }

    fn render_advanced_observation(
        &self,
        handle: &str,
        observation: AdvancedObservationKind,
    ) -> Result<Value, DeveloperLoopError> {
        match observation {
            AdvancedObservationKind::Terminal(terminal) => {
                result::terminal(
                    handle,
                    &terminal.result,
                    &self.inner.artifacts,
                    &terminal.progress,
                )
                .map_err(tool)
            }
            AdvancedObservationKind::Recovered(value) => {
                result::materialize_deferred(value, &self.inner.artifacts, handle).map_err(tool)
            }
            AdvancedObservationKind::Starting { process_id, interactive, elapsed_millis } => Ok(
                result::starting(handle, process_id, interactive, elapsed_millis),
            ),
            AdvancedObservationKind::Active(progress) => Ok(result::active(handle, &progress)),
            AdvancedObservationKind::OperationFailed(error) => {
                Ok(result::operation_failed(handle, &error))
            }
            AdvancedObservationKind::SettlementPending(failure, progress) => {
                Ok(result::settlement_pending(handle, &failure, &progress))
            }
        }
    }

    pub(super) fn reconcile_observer(
        &self,
        handle: &str,
    ) -> Result<bool, DeveloperLoopError> {
        let observation = match self.advance_observation(handle, Observation::Poll) {
            Ok(advanced) => advanced.observation,
            Err(error) => {
                if self.observer_reconciliation_settled(handle)? {
                    return Ok(true);
                }
                return Err(error);
            }
        };
        match observation {
            AdvancedObservationKind::Terminal(terminal) => {
                let deferred = with_execution_context(
                    result::terminal_deferred(handle, &terminal.result, &terminal.progress),
                    Some(terminal.mode),
                    terminal.resource_evidence.as_ref(),
                );
                self.record_deferred_projection(handle, deferred)?;
                self.publish_replay_action(terminal.result.action_id());
            }
            AdvancedObservationKind::Recovered(value) => {
                self.record_projection(handle, value)?;
                if let Some(action_id) = self.pending_replay_action_for_handle(handle)? {
                    self.publish_replay_action(action_id);
                }
            }
            AdvancedObservationKind::OperationFailed(error) => return Err(tool(error.to_string())),
            AdvancedObservationKind::Starting { .. }
            | AdvancedObservationKind::Active(_)
            | AdvancedObservationKind::SettlementPending(_, _) => {}
        }
        self.observer_reconciliation_settled(handle)
    }

    fn enqueue_observer_reconciliation(
        &self,
        handle: &str,
        control: Option<peritus_process::ProcessControl>,
    ) {
        self.inner.cancellation_worker.enqueue_observer_reconciliation(
            self.clone(),
            handle.to_owned(),
            control,
        );
    }

    fn retain_projection(&self, handle: &str, value: Value) {
        if let Err(error) = self.record_projection(handle, value) {
            crate::diagnostic::report(&format!(
                "peritus command runtime: command {handle} completed an in-memory transition, but its reconnect projection could not be retained: {error}"
            ));
        }
    }

    fn record_projection(&self, handle: &str, value: Value) -> Result<(), DeveloperLoopError> {
        let _guard = self
            .inner
            .projection_lock
            .lock()
            .map_err(|_| tool("command projection writer is poisoned"))?;
        projections::record(&self.inner.state_root, handle, value).map_err(tool)
    }

    fn record_deferred_projection(
        &self,
        handle: &str,
        value: Value,
    ) -> Result<(), DeveloperLoopError> {
        let _guard = self
            .inner
            .projection_lock
            .lock()
            .map_err(|_| tool("command projection writer is poisoned"))?;
        projections::record_deferred(&self.inner.state_root, handle, value)
            .map(|_| ())
            .map_err(tool)
    }

    pub(super) fn observer_reconciliation_settled(
        &self,
        handle: &str,
    ) -> Result<bool, DeveloperLoopError> {
        let state = self.inner.state.lock().map_err(|_| tool("command runtime is poisoned"))?;
        if state.starting.contains_key(handle) || state.active.contains_key(handle) {
            return Ok(false);
        }
        if let Some(terminal) = state.terminal.get(handle) {
            return Ok(!state.replay_pending.contains_key(&terminal.result.action_id()));
        }
        if let Some(recovered) = state.recovered.get(handle) {
            let final_projection = matches!(
                recovered.get("state").and_then(Value::as_str),
                Some("completed" | "indeterminate")
            );
            let publication_pending = state
                .replay_pending
                .keys()
                .any(|action_id| identity::action_hex(*action_id) == handle);
            return Ok(final_projection && !publication_pending);
        }
        Err(tool("command invocation handle is unknown"))
    }

    fn pending_replay_action_for_handle(
        &self,
        handle: &str,
    ) -> Result<Option<ActionId>, DeveloperLoopError> {
        let state = self.inner.state.lock().map_err(|_| tool("command runtime is poisoned"))?;
        Ok(state
            .replay_pending
            .keys()
            .copied()
            .find(|action_id| identity::action_hex(*action_id) == handle))
    }
}

fn with_execution_mode(value: Value, mode: CommandExecutionMode) -> Value {
    with_execution_context(value, Some(mode), None)
}

fn with_execution_context(
    mut value: Value,
    mode: Option<CommandExecutionMode>,
    resource_evidence: Option<&Value>,
) -> Value {
    if let Some(object) = value.as_object_mut() {
        if let Some(mode) = mode {
            object.insert(
                "execution_mode".to_owned(),
                Value::String(mode.label().to_owned()),
            );
        }
        if let Some(resource_evidence) = resource_evidence {
            object.insert("execution_resources".to_owned(), resource_evidence.clone());
        }
    }
    value
}

fn execution_mode_from_value(value: &Value) -> Option<CommandExecutionMode> {
    match value.get("execution_mode").and_then(Value::as_str) {
        Some("observational") => Some(CommandExecutionMode::Observational),
        Some("mutation") => Some(CommandExecutionMode::Mutation),
        _ => None,
    }
}

fn observed_update(update: ExecutionUpdate) -> ObservedCommand {
    let progress = ProgressBatch {
        events: update.progress().to_vec(),
        page: update.progress_page().cloned(),
    };
    if let Some(terminal) = update.terminal().cloned() {
        ObservedCommand::Terminal(terminal, progress)
    } else if let Some(failure) = update.settlement_failure().cloned() {
        ObservedCommand::SettlementPending(failure, progress)
    } else {
        ObservedCommand::Active(progress)
    }
}

fn active_owner_matches(
    state: &RuntimeState,
    handle: &str,
    router: &CommandRouter,
    invocation: InvocationHandle,
) -> bool {
    state.active.get(handle).is_some_and(|active| {
        active.invocation == invocation && Arc::ptr_eq(&active.router, router)
    })
}

fn starting_owner_matches(
    state: &RuntimeState,
    handle: &str,
    router: &CommandRouter,
) -> bool {
    state
        .starting
        .get(handle)
        .is_some_and(|starting| Arc::ptr_eq(&starting.router, router))
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

fn ensure_not_cancelled(cancellation: &CancellationToken) -> Result<(), DeveloperLoopError> {
    if cancellation.is_cancelled() {
        Err(DeveloperLoopError::Cancelled)
    } else {
        Ok(())
    }
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
