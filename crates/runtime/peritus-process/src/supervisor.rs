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
    control::{SharedExecution, SharedObservation},
    events::EventLog,
    gateway::AuthorizedLaunch,
    output::SpoolSet,
    platform,
};

mod artifact;
mod finalization;
mod io;
mod owner;
mod ownership;
mod resource;

use artifact::publish_spools;
use finalization::publish_spawn_failure;
use owner::SpawnedOwner;
pub(crate) use resource::validate_launch;

const CONTROL_QUEUE: usize = 64;
const OUTPUT_QUEUE: usize = 64;
const POLL_MILLIS: u64 = 5;

/// Move-only owner of one supervisor thread and complete process lifecycle.
#[must_use = "the owned process must be waited or dropped for bounded cancellation and join"]
pub struct OwnedProcess {
    store: ProcessStore,
    control: ProcessControl,
    join: Option<JoinHandle<Result<TerminalResult, ProcessError>>>,
    spool_directory: PathBuf,
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
        publish_spools(
            &self.store,
            result.process_id(),
            &self.spool_directory,
            artifacts,
            creating_event,
        )
    }

    fn join_owner(&mut self) -> Result<TerminalResult, ProcessError> {
        let join =
            self.join.take().ok_or_else(|| supervisor_error("process owner was already joined"))?;
        join.join().map_err(|_| supervisor_error("process owner thread panicked"))?
    }
}

impl Drop for OwnedProcess {
    fn drop(&mut self) {
        let Some(join) = self.join.take() else {
            return;
        };
        let _ = self.control.cancel(CancellationReason::SupervisorShutdown);
        let _ = join.join();
    }
}

pub(crate) fn start(
    store: &ProcessStore,
    launch: AuthorizedLaunch,
) -> Result<OwnedProcess, ProcessError> {
    let (plan, _action_digest) = launch.into_parts();
    let process_id = plan.identity().process_id();
    store.record_phase(process_id, LifecyclePhase::Starting)?;
    let spool_directory = store.spool_directory(process_id)?;
    let (control_tx, control_rx) = mpsc::sync_channel(CONTROL_QUEUE);
    let shared = Arc::new(SharedObservation {
        state: std::sync::Mutex::new(SharedExecution {
            events: EventLog::new(plan.output_policy().event_count()),
            retained_output: Vec::new(),
            terminal: None,
        }),
        changed: std::sync::Condvar::new(),
    });
    emit(&shared, &plan, None, ProcessEventKind::IntentPersisted, Vec::new());
    let control = ProcessControl::new(
        control_tx,
        Arc::clone(&shared),
        plan.stdin_policy(),
        plan.terminal_capabilities(),
    );
    let thread_store = store.clone();
    let thread_shared = Arc::clone(&shared);
    let thread_spool = spool_directory.clone();
    let thread_plan = plan.clone();
    let name = format!("peritus-process-{}", short_id(process_id.as_bytes()));
    let Ok(join) = thread::Builder::new().name(name).spawn(move || {
        run_owner(&thread_store, &thread_plan, &thread_spool, control_rx, thread_shared)
    }) else {
        let error = supervisor_error("process owner thread cannot be created");
        let _ = publish_spawn_failure(store, &plan, &shared, Instant::now(), error);
        return Err(supervisor_error("process owner thread cannot be created"));
    };
    Ok(OwnedProcess { store: store.clone(), control, join: Some(join), spool_directory })
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
        let directory = self.spool_directory(process_id).map_err(WaitAndPublishError::owner)?;
        publish_spools(self, process_id, &directory, artifacts, creating_event)
    }
}

fn run_owner(
    store: &ProcessStore,
    plan: &ExecutionPlan,
    spool_directory: &std::path::Path,
    control_rx: mpsc::Receiver<crate::control::ControlCommand>,
    shared: Arc<SharedObservation>,
) -> Result<TerminalResult, ProcessError> {
    let began = Instant::now();
    emit(&shared, plan, None, ProcessEventKind::SpawnAttempt, Vec::new());
    let spools = match plan.io_mode() {
        crate::IoMode::Pipes => {
            SpoolSet::pipes(spool_directory, plan.output_policy().spool_bytes())
        }
        crate::IoMode::Pty(_) => SpoolSet::pty(spool_directory, plan.output_policy().spool_bytes()),
    };
    let spools = match spools {
        Ok(spools) => spools,
        Err(error) => return publish_spawn_failure(store, plan, &shared, began, error),
    };
    let resources = match resource::ResourceTracker::start(plan) {
        Ok(resources) => resources,
        Err(error) => return publish_spawn_failure(store, plan, &shared, began, error),
    };
    let process = match platform::launch(plan) {
        Ok(process) => process,
        Err(error) => return publish_spawn_failure(store, plan, &shared, began, error),
    };
    let initial_failure = !process.identity().complete_containment();
    SpawnedOwner::new(
        store.clone(),
        plan.clone(),
        control_rx,
        shared,
        process,
        spools,
        resources,
        began,
    )
    .run(initial_failure)
}

pub(super) fn publish_terminal(
    shared: &Arc<SharedObservation>,
    plan: &ExecutionPlan,
    result: &TerminalResult,
) {
    let mut state = shared.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    state.events.push(
        plan.identity().process_id(),
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
    plan: &ExecutionPlan,
    offset: Option<u64>,
    kind: ProcessEventKind,
    data: Vec<u8>,
) -> u64 {
    let mut state = shared.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let sequence =
        state.events.push(plan.identity().process_id(), plan.digest(), offset, kind, data);
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
