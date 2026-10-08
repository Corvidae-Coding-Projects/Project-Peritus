//! Owned supervisor thread for process, I/O, cancellation, and terminal publication.

use std::{
    path::PathBuf,
    sync::{Arc, mpsc},
    thread::{self, JoinHandle},
    time::Instant,
};

use peritus_artifact_store::ArtifactStore;
use peritus_types::EventId;

use crate::{
    CancellationReason, ErrorCode, ExecutionPlan, LifecyclePhase, ProcessControl, ProcessError,
    ProcessEventKind, ProcessOperation, ProcessStore, RecoveryClass, TerminalResult,
    control::{
        CancellationOwner, InputLane, InputOwner, SharedExecution, SharedObservation,
        cancellation_lane, input_lane,
    },
    events::EventLog,
    gateway::AuthorizedLaunch,
    native::NativeSandboxSession,
    output::SpoolSet,
    platform,
    retained_owner::{RetainedProcessKey, RetainedProcessTransport},
};

mod artifact;
mod finalization;
mod io;
mod owner;
mod ownership;
mod plan;
mod resource;

use artifact::publish_spools;
use finalization::publish_spawn_failure;
pub(crate) use finalization::{record_preparation_cancellation, record_preparation_failure};
use owner::SpawnedOwner;
pub(crate) use plan::SupervisorPlan;
pub(crate) use resource::validate_launch;
pub(crate) use resource::validate_native_launch;

const CONTROL_QUEUE: usize = 64;
const OUTPUT_QUEUE: usize = 64;
const POLL_MILLIS: u64 = 5;

/// Move-only owner of one supervisor thread and complete process lifecycle.
#[must_use = "the owned process must be waited or dropped for bounded cancellation and join"]
pub struct OwnedProcess {
    store: ProcessStore,
    control: ProcessControl,
    owner: Option<ProcessOwner>,
    spool_directory: PathBuf,
}

enum ProcessOwner {
    Local(JoinHandle<Result<TerminalResult, ProcessError>>),
    Retained {
        transport: Arc<dyn RetainedProcessTransport>,
        key: RetainedProcessKey,
    },
}

impl OwnedProcess {
    /// Returns a cloneable bounded control/observation handle.
    #[must_use]
    pub fn control(&self) -> ProcessControl {
        self.control.clone()
    }

    /// Waits for the unique terminal result and joins the owning supervisor.
    ///
    /// # Errors
    ///
    /// Returns a typed supervisor error when the owner thread failed before terminal publication.
    pub fn wait(mut self) -> Result<TerminalResult, ProcessError> {
        self.join_owner()
    }

    /// Waits, then publishes every nonempty retained output spool into the C0 artifact store.
    ///
    /// # Errors
    ///
    /// Returns a typed error while retaining the spool for explicit retry. Publication failures
    /// carry the latest durable process terminal result and leave it available through any
    /// previously cloned [`ProcessControl`].
    pub fn wait_and_publish(
        mut self,
        artifacts: &ArtifactStore,
        creating_event: EventId,
    ) -> Result<TerminalResult, WaitAndPublishError> {
        let result = self.join_owner().map_err(WaitAndPublishError::owner)?;
        self.store
            .refresh_authoritative_identity(result.process_id())
            .map_err(|error| WaitAndPublishError::publication(result.clone(), error))?;
        publish_spools(
            &self.store,
            result.process_id(),
            &self.spool_directory,
            artifacts,
            creating_event,
        )
    }

    /// Waits and publishes retained output while cooperatively observing caller cancellation.
    ///
    /// Once cancellation is observed, this queues an exact stop request and still joins the
    /// owning supervisor. Cleanup failure therefore remains an honest unresolved durable process
    /// state instead of being hidden by abandoning the owner.
    ///
    /// # Errors
    /// Returns a typed owner or artifact-publication failure. A terminal result remains attached
    /// when only artifact publication failed.
    pub fn wait_and_publish_cancellable(
        self,
        artifacts: &ArtifactStore,
        creating_event: EventId,
        mut cancellation_requested: impl FnMut() -> bool,
    ) -> Result<TerminalResult, WaitAndPublishError> {
        self.wait_and_publish_with_cancellation(artifacts, creating_event, || {
            cancellation_requested().then_some(CancellationReason::User)
        })
    }

    /// Waits and publishes retained output while admitting the caller's exact cancellation cause.
    ///
    /// Returning `Some` admits that reason once and still joins the unique process owner. This
    /// keeps authority revocation, service shutdown, and user cancellation distinct in the
    /// durable terminal result.
    ///
    /// # Errors
    /// Returns a typed owner or artifact-publication failure. A terminal result remains attached
    /// when only artifact publication failed.
    pub fn wait_and_publish_with_cancellation(
        mut self,
        artifacts: &ArtifactStore,
        creating_event: EventId,
        mut cancellation_requested: impl FnMut() -> Option<CancellationReason>,
    ) -> Result<TerminalResult, WaitAndPublishError> {
        let mut cancellation_sent = false;
        loop {
            if self
                .control
                .try_owner_finished()
                .map_err(WaitAndPublishError::owner)?
            {
                break;
            }
            if !cancellation_sent
                && let Some(reason) = cancellation_requested()
            {
                match self.control.cancel_while(reason, || true) {
                    Ok(sent) => cancellation_sent = sent,
                    Err(error) => {
                        if self
                            .control
                            .try_owner_finished()
                            .map_err(WaitAndPublishError::owner)?
                        {
                            break;
                        }
                        return Err(WaitAndPublishError::owner(error));
                    }
                }
            }
            thread::sleep(std::time::Duration::from_millis(POLL_MILLIS));
        }
        let result = self.join_owner().map_err(WaitAndPublishError::owner)?;
        self.store
            .refresh_authoritative_identity(result.process_id())
            .map_err(|error| WaitAndPublishError::publication(result.clone(), error))?;
        publish_spools(
            &self.store,
            result.process_id(),
            &self.spool_directory,
            artifacts,
            creating_event,
        )
    }

    fn join_owner(&mut self) -> Result<TerminalResult, ProcessError> {
        match self
            .owner
            .take()
            .ok_or_else(|| supervisor_error("process owner was already joined"))?
        {
            ProcessOwner::Local(join) => {
                join.join().map_err(|_| supervisor_error("process owner thread panicked"))?
            }
            ProcessOwner::Retained { transport, key } => transport.wait(key),
        }
    }
}

impl Drop for OwnedProcess {
    fn drop(&mut self) {
        let Some(owner) = self.owner.take() else {
            return;
        };
        if let ProcessOwner::Local(join) = owner {
            let _ = self.control.cancel_while(CancellationReason::SupervisorShutdown, || {
                !join.is_finished()
            });
            let _ = join.join();
        }
        // Dropping a retained observer only detaches this daemon generation. The independent
        // service owner continues the exact process and accepts a later same-key attachment.
    }
}

pub(crate) fn attach_retained(
    store: &ProcessStore,
    transport: Arc<dyn RetainedProcessTransport>,
    key: RetainedProcessKey,
    terminal: crate::TerminalCapabilities,
) -> Result<OwnedProcess, ProcessError> {
    let spool_directory = store.spool_directory(key.process_id())?;
    let control = ProcessControl::new_retained(Arc::clone(&transport), key, terminal);
    Ok(OwnedProcess {
        store: store.clone(),
        control,
        owner: Some(ProcessOwner::Retained { transport, key }),
        spool_directory,
    })
}

pub(crate) fn start(
    store: &ProcessStore,
    launch: AuthorizedLaunch,
) -> Result<OwnedProcess, ProcessError> {
    start_with_native(store, launch, None, None)
}

pub(crate) fn start_native(
    store: &ProcessStore,
    launch: AuthorizedLaunch,
    session: Box<dyn NativeSandboxSession>,
    sandbox_digest: peritus_types::Sha256Digest,
) -> Result<OwnedProcess, ProcessError> {
    start_with_native(store, launch, Some(session), Some(sandbox_digest))
}

fn start_with_native(
    store: &ProcessStore,
    launch: AuthorizedLaunch,
    session: Option<Box<dyn NativeSandboxSession>>,
    sandbox_digest: Option<peritus_types::Sha256Digest>,
) -> Result<OwnedProcess, ProcessError> {
    let native_recovery = session
        .as_deref()
        .map(NativeSandboxSession::recovery_snapshot)
        .transpose()?
        .flatten();
    let (execution_plan, _action_digest) = launch.into_parts();
    let process_id = execution_plan.identity().process_id();
    let plan = SupervisorPlan::from_execution(&execution_plan);
    store.record_phase(process_id, LifecyclePhase::Starting)?;
    let spool_directory = store.spool_directory(process_id)?;
    let (control_tx, control_rx) = mpsc::sync_channel(CONTROL_QUEUE);
    let (cancellation, cancellation_owner) = cancellation_lane();
    let (input, input_owner) = input_lane(plan.stdin_policy());
    let shared = Arc::new(SharedObservation {
        state: std::sync::Mutex::new(SharedExecution {
            events: EventLog::new(plan.output_policy().event_count()),
            retained_stdout: Vec::new(),
            retained_stderr: Vec::new(),
            retained_terminal: Vec::new(),
            tree: None,
            native_recovery,
            terminal: None,
        }),
        changed: std::sync::Condvar::new(),
    });
    emit(&shared, &plan, None, ProcessEventKind::IntentPersisted, Vec::new());
    let control = ProcessControl::new(
        control_tx,
        cancellation,
        input.clone(),
        Arc::clone(&shared),
        plan.terminal_capabilities(),
    );
    let pending_session = Arc::new(std::sync::Mutex::new(session));
    let thread_session = Arc::clone(&pending_session);
    let thread_store = store.clone();
    let thread_shared = Arc::clone(&shared);
    let thread_spool = spool_directory.clone();
    let thread_plan = plan.clone();
    let name = format!("peritus-process-{}", short_id(process_id.as_bytes()));
    let Ok(join) = thread::Builder::new().name(name).spawn(move || {
        let session =
            thread_session.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take();
        run_owner(
            &thread_store,
            &thread_plan,
            &thread_spool,
            control_rx,
            cancellation_owner,
            input_owner,
            input,
            thread_shared,
            session,
            sandbox_digest,
        )
    }) else {
        let error = supervisor_error("process owner thread cannot be created");
        let cleanup_complete = release_pre_spawn_session(
            store,
            &shared,
            &mut pending_session.lock().unwrap_or_else(std::sync::PoisonError::into_inner),
            &plan,
            sandbox_digest,
        );
        let _ =
            publish_spawn_failure(store, &plan, &shared, Instant::now(), cleanup_complete, error);
        return Err(supervisor_error("process owner thread cannot be created"));
    };
    Ok(OwnedProcess {
        store: store.clone(),
        control,
        owner: Some(ProcessOwner::Local(join)),
        spool_directory,
    })
}

/// A wait or artifact-publication failure with any durable terminal result preserved.
#[derive(Debug)]
pub struct WaitAndPublishError {
    terminal: Option<Box<TerminalResult>>,
    source: ProcessError,
}

impl WaitAndPublishError {
    const fn owner(source: ProcessError) -> Self {
        Self { terminal: None, source }
    }

    fn publication(terminal: TerminalResult, source: ProcessError) -> Self {
        Self { terminal: Some(Box::new(terminal)), source }
    }

    /// Returns the completed durable process result when only artifact publication failed.
    #[must_use]
    pub fn terminal_result(&self) -> Option<&TerminalResult> {
        self.terminal.as_deref()
    }

    /// Returns the stable underlying process or publication failure.
    #[must_use]
    pub const fn process_error(&self) -> &ProcessError {
        &self.source
    }
}

impl core::fmt::Display for WaitAndPublishError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.source.fmt(formatter)
    }
}

impl std::error::Error for WaitAndPublishError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

impl ProcessStore {
    /// Retries retained-output publication from durable terminal state and process spools.
    ///
    /// Already-published stream records are skipped, and each newly finalized stream is persisted
    /// before the next stream begins.
    ///
    /// # Errors
    ///
    /// Returns a typed wait/publication error. Once a terminal record is available, the error
    /// preserves its latest durable result for callers.
    pub fn retry_artifact_publication(
        &self,
        process_id: peritus_types::ProcessId,
        artifacts: &ArtifactStore,
        creating_event: EventId,
    ) -> Result<TerminalResult, WaitAndPublishError> {
        self.refresh_authoritative_identity(process_id)
            .map_err(WaitAndPublishError::owner)?;
        let directory = self.spool_directory(process_id).map_err(WaitAndPublishError::owner)?;
        publish_spools(self, process_id, &directory, artifacts, creating_event)
    }
}

fn run_owner(
    store: &ProcessStore,
    plan: &SupervisorPlan,
    spool_directory: &std::path::Path,
    control_rx: mpsc::Receiver<crate::control::ControlCommand>,
    cancellation: CancellationOwner,
    input_owner: InputOwner,
    input_lane: InputLane,
    shared: Arc<SharedObservation>,
    mut native: Option<Box<dyn NativeSandboxSession>>,
    sandbox_digest: Option<peritus_types::Sha256Digest>,
) -> Result<TerminalResult, ProcessError> {
    let began = Instant::now();
    emit(&shared, plan, None, ProcessEventKind::SpawnAttempt, Vec::new());
    let spools = match plan.io_mode() {
        crate::IoMode::Pipes => {
            SpoolSet::pipes(spool_directory, plan.output_policy().spool_segment_bytes())
        }
        crate::IoMode::Pty(_) => {
            SpoolSet::pty(spool_directory, plan.output_policy().spool_segment_bytes())
        }
    };
    let spools = match spools {
        Ok(spools) => spools,
        Err(error) => {
            let cleanup_complete =
                release_pre_spawn_session(store, &shared, &mut native, plan, sandbox_digest);
            return publish_spawn_failure(store, plan, &shared, began, cleanup_complete, error);
        }
    };
    let resources = match resource::ResourceTracker::start(plan) {
        Ok(resources) => resources,
        Err(error) => {
            let cleanup_complete =
                release_pre_spawn_session(store, &shared, &mut native, plan, sandbox_digest);
            return publish_spawn_failure(store, plan, &shared, began, cleanup_complete, error);
        }
    };
    let launch_description = native.as_deref().map(NativeSandboxSession::launch_description);
    let launch_command = launch_description
        .map_or_else(|| plan.command(), |value| value.command())
        .clone();
    let handshake = launch_description.map(|value| platform::NativeHandshake {
        manifest: value.manifest_pages().map(<[u8]>::to_vec).collect(),
        ready: value.ready_record(),
        activated: value.activation_record(),
        #[cfg(windows)]
        started: crate::native_target_started_record(
            value.manifest_digest(),
            value.preparation_digest(),
        ),
        protected_handles: value.protected_handles().to_vec(),
        #[cfg(windows)]
        windows_channels: value.windows_helper_channels().cloned(),
    });
    let launch = {
        let mut record_spawned = |tree| {
            #[cfg(target_os = "macos")]
            store.record_spawned(plan.process_id(), tree)?;
            if let Some(session) = native.as_deref_mut() {
                session.spawned(tree)?;
                publish_native_recovery(&shared, session)?;
            }
            Ok(())
        };
        let mut should_continue = || cancellation.pending().is_none();
        platform::launch(
            plan,
            &launch_command,
            handshake,
            store.crash_watchdog(),
            &mut record_spawned,
            &mut should_continue,
        )
    };
    let launch = match launch {
        Ok(launch) => launch,
        Err(error) => {
            let cleanup_complete =
                release_pre_spawn_session(store, &shared, &mut native, plan, sandbox_digest);
            return publish_spawn_failure(store, plan, &shared, began, cleanup_complete, error);
        }
    };
    let (process, handshake_status) = launch.into_parts();
    let mut pre_start_reason = match handshake_status {
        platform::NativeHandshakeStatus::Complete => None,
        platform::NativeHandshakeStatus::Cancelled | platform::NativeHandshakeStatus::Failed => {
            Some(cancellation.pending().unwrap_or(CancellationReason::BackendFailure))
        }
    };
    let mut initial_failure = !process.identity().complete_containment()
        || matches!(handshake_status, platform::NativeHandshakeStatus::Failed);
    if !process.identity().complete_containment() {
        pre_start_reason.get_or_insert(CancellationReason::BackendFailure);
    }
    if matches!(handshake_status, platform::NativeHandshakeStatus::Complete)
        && let Some(session) = native.as_deref_mut()
    {
        let activation = {
            let mut should_continue = || cancellation.pending().is_none();
            session.activated_while(process.identity(), &mut should_continue)
        };
        if activation.is_ok() {
            if crate::native::capture_activated_session(
                store,
                session,
                plan,
                plan.sandbox_digest(),
            )
            .is_err()
            {
                initial_failure = true;
                pre_start_reason.get_or_insert(CancellationReason::BackendFailure);
            } else if publish_native_recovery(&shared, session).is_err() {
                initial_failure = true;
                pre_start_reason.get_or_insert(CancellationReason::BackendFailure);
            }
        } else if let Some(reason) = cancellation.pending() {
            pre_start_reason = Some(reason);
        } else {
            initial_failure = true;
            pre_start_reason = Some(CancellationReason::BackendFailure);
        }
    }
    SpawnedOwner::new(
        store.clone(),
        plan.clone(),
        control_rx,
        cancellation,
        input_owner,
        input_lane,
        shared,
        process,
        spools,
        resources,
        began,
        native,
    )
    .run(initial_failure, pre_start_reason)
}

fn release_pre_spawn_session(
    store: &ProcessStore,
    shared: &Arc<SharedObservation>,
    session: &mut Option<Box<dyn NativeSandboxSession>>,
    plan: &SupervisorPlan,
    sandbox_digest: Option<peritus_types::Sha256Digest>,
) -> bool {
    let Some(session) = session.as_deref_mut() else {
        return true;
    };
    let Some(sandbox_digest) = sandbox_digest else {
        return false;
    };
    let release = session.release();
    let capture =
        crate::native::capture_released_session(store, session, plan, sandbox_digest);
    let recovery = publish_native_recovery(shared, session);
    release.is_ok() && capture.is_ok() && recovery.is_ok()
}

pub(super) fn publish_native_recovery(
    shared: &Arc<SharedObservation>,
    session: &dyn NativeSandboxSession,
) -> Result<(), ProcessError> {
    let recovery = session.recovery_snapshot()?;
    let mut state = shared.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    state.native_recovery = recovery;
    drop(state);
    shared.changed.notify_all();
    Ok(())
}

pub(super) fn publish_terminal(
    shared: &Arc<SharedObservation>,
    plan: &SupervisorPlan,
    result: &TerminalResult,
) {
    let mut state = shared.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    state.events.push(
        plan.process_id(),
        plan.digest(),
        None,
        ProcessEventKind::TerminalPublished,
        Vec::new(),
    );
    state.terminal = Some(result.clone());
    drop(state);
    shared.changed.notify_all();
}

pub(super) fn emit(
    shared: &Arc<SharedObservation>,
    plan: &SupervisorPlan,
    offset: Option<u64>,
    kind: ProcessEventKind,
    data: Vec<u8>,
) -> u64 {
    let mut state = shared.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let sequence =
        state.events.push(plan.process_id(), plan.digest(), offset, kind, data);
    drop(state);
    shared.changed.notify_all();
    sequence
}

pub(super) fn elapsed_millis(since: Instant) -> u64 {
    u64::try_from(since.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn short_id(bytes: &[u8; 16]) -> String {
    let mut result = String::with_capacity(8);
    for byte in &bytes[..4] {
        use core::fmt::Write as _;
        write!(&mut result, "{byte:02x}").expect("writing to String is infallible");
    }
    result
}

pub(super) const fn supervisor_error(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::Supervisor,
        ProcessOperation::Wait,
        RecoveryClass::ReopenAndReconcile,
        detail,
    )
}
