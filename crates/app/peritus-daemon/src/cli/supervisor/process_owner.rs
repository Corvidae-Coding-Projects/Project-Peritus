//! Service-lifetime owner of retained native process sessions.

mod factory;
mod client;
mod protocol;

pub(super) use client::OwnerClient;

use std::{
    collections::BTreeMap,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use peritus_process::{
    CancellationReason, ControlRejection, ErrorCode, ExecutionGateway, LifecyclePhase,
    NativeSandboxBackend, OutputStream, OwnedProcess, ProcessControl, ProcessCursor, ProcessError,
    ProcessOperation, ProcessSignal, ProcessStore, RecoveryClass, RetainedOwnerObservation,
    RetainedCompletionBinding, RetainedOwnerCompletionStatus, RetainedOwnerRequest,
    RetainedProcessKey, RetainedStreamPage, RetainedStreamSnapshot, TerminalResult, TerminalSize,
};
use peritus_sandbox::BackendAdmission;
use peritus_types::ProcessId;

use crate::DaemonConfig;

use factory::reconstruct_backend;

struct RetainedExecution {
    key: RetainedProcessKey,
    request_digest: peritus_types::Sha256Digest,
    control: ProcessControl,
    terminal: Option<TerminalResult>,
    failure: Option<RetainedFailure>,
    publication_failure: Option<RetainedFailure>,
    completion_retrying: bool,
    stream_snapshots: [Option<RetainedStreamSnapshot>; 3],
    snapshot_captures: [bool; 3],
    windows_owner_identity: Option<peritus_types::Sha256Digest>,
}

#[derive(Default)]
struct SharedProcessStore {
    value: Option<ProcessStore>,
    opening: bool,
}

struct RetainedLaunch {
    key: RetainedProcessKey,
    request_digest: peritus_types::Sha256Digest,
    cancellation: Arc<LaunchCancellation>,
    outcome: Option<RetainedLaunchOutcome>,
}

enum RetainedLaunchOutcome {
    Published,
    Terminal {
        terminal: TerminalResult,
        publication_failure: RetainedFailure,
        completion_retrying: bool,
    },
    Failed(RetainedFailure),
}

enum LaunchCompletion {
    Published,
    Terminal(TerminalResult),
}

enum LaunchWork {
    Native(Arc<LaunchCancellation>),
    Completion(TerminalResult),
}

struct ExecutionCompletionTask {
    key: RetainedProcessKey,
    request_digest: peritus_types::Sha256Digest,
    terminal: TerminalResult,
    control: ProcessControl,
    outstanding: [Option<RetainedStreamSnapshot>; 3],
}

enum SnapshotAction {
    Ready(RetainedStreamSnapshot),
    CaptureOutstanding(ProcessControl),
    CaptureFinal(ProcessControl),
}

#[derive(Default)]
struct LaunchCancellation {
    state: Mutex<LaunchCancellationState>,
}

#[derive(Default)]
struct LaunchCancellationState {
    first: Option<CancellationReason>,
    control: Option<ProcessControl>,
}

impl LaunchCancellation {
    fn reason(&self) -> Option<CancellationReason> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .first
    }

    fn request(&self, reason: CancellationReason) -> Result<(), ProcessError> {
        let (first, control) = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let first = *state.first.get_or_insert(reason);
            (first, state.control.clone())
        };
        control.map_or(Ok(()), |control| control.cancel(first))
    }

    fn publish(&self, control: ProcessControl) -> Result<(), ProcessError> {
        let reason = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.control = Some(control.clone());
            state.first
        };
        reason.map_or(Ok(()), |reason| control.cancel(reason))
    }
}

const RETAINED_EVENT_PAGE: usize = 256;
const RETAINED_STREAM_PAGE_BYTES: usize = 64 * 1_024;

#[derive(Clone, Copy)]
struct RetainedFailure {
    code: ErrorCode,
    operation: ProcessOperation,
    recovery: RecoveryClass,
    rejection: Option<ControlRejection>,
}

impl RetainedFailure {
    const fn capture(error: &ProcessError) -> Self {
        Self {
            code: error.code(),
            operation: error.operation(),
            recovery: error.recovery(),
            rejection: error.control_rejection(),
        }
    }

    const fn restore(self) -> ProcessError {
        ProcessError::from_retained_owner(
            self.code,
            self.operation,
            self.recovery,
            self.rejection,
        )
    }
}

/// Native process owner whose lifetime is the service supervisor, not one daemon attempt.
pub(super) struct ProcessOwner {
    config: DaemonConfig,
    service_owner: peritus_process::RetainedServiceOwner,
    store: Mutex<SharedProcessStore>,
    store_changed: Condvar,
    accepting: Arc<AtomicBool>,
    executions: Mutex<BTreeMap<ProcessId, RetainedExecution>>,
    execution_changed: Condvar,
    launches: Mutex<BTreeMap<ProcessId, RetainedLaunch>>,
    launch_boundary: Mutex<()>,
    launch_workers: Mutex<Vec<JoinHandle<()>>>,
    completion_workers: Mutex<Vec<JoinHandle<()>>>,
}

impl ProcessOwner {
    pub(super) fn new(config: DaemonConfig, owner_token: &[u8]) -> Self {
        Self {
            config,
            service_owner: peritus_process::RetainedServiceOwner::from_token(owner_token),
            store: Mutex::new(SharedProcessStore::default()),
            store_changed: Condvar::new(),
            accepting: Arc::new(AtomicBool::new(true)),
            executions: Mutex::new(BTreeMap::new()),
            execution_changed: Condvar::new(),
            launches: Mutex::new(BTreeMap::new()),
            launch_boundary: Mutex::new(()),
            launch_workers: Mutex::new(Vec::new()),
            completion_workers: Mutex::new(Vec::new()),
        }
    }

    pub(super) const fn service_owner(&self) -> peritus_process::RetainedServiceOwner {
        self.service_owner
    }

    pub(super) fn launch_or_attach(
        self: &Arc<Self>,
        key: RetainedProcessKey,
        request_bytes: Vec<u8>,
        request_digest: peritus_types::Sha256Digest,
    ) -> Result<bool, ProcessError> {
        self.reap_launch_workers();
        self.reap_completion_workers();
        let request = RetainedOwnerRequest::decode(request_bytes)?;
        let binding = request.binding();
        if request.digest() != request_digest
            || RetainedProcessKey::from_binding(binding) != key
            || binding.service_owner() != self.service_owner
        {
            return Err(owner_error(
                ErrorCode::AuthorizationMismatch,
                RecoveryClass::Quarantine,
                "retained owner request differs from the authenticated service generation",
            ));
        }
        if let Some(ready) = self.retry_or_attach_execution(key, request_digest)? {
            return Ok(ready);
        }
        let launch_boundary = self
            .launch_boundary
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let stopping = !self.accepting.load(Ordering::Acquire);
        let work = {
            let mut launches = self
                .launches
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(existing) = launches.get_mut(&key.process_id()) {
                if existing.key != key || existing.request_digest != request_digest {
                    return Err(owner_error(
                        ErrorCode::ReceiptReused,
                        RecoveryClass::Quarantine,
                        "retained process identity is already bound to another launch",
                    ));
                }
                if stopping {
                    let _ = existing
                        .cancellation
                        .request(CancellationReason::SupervisorShutdown);
                }
                match existing.outcome.as_mut() {
                    Some(RetainedLaunchOutcome::Failed(_)) => {
                        // The worker revalidates the canonical reservation before every attempt.
                        // Only an Authorized identity can pass that gate, so a failure before
                        // native preparation may recover after a trusted dependency is restored.
                        existing.outcome = None;
                        LaunchWork::Native(Arc::clone(&existing.cancellation))
                    }
                    Some(RetainedLaunchOutcome::Terminal {
                        terminal,
                        completion_retrying,
                        ..
                    }) if !*completion_retrying => {
                        *completion_retrying = true;
                        LaunchWork::Completion(terminal.clone())
                    }
                    Some(RetainedLaunchOutcome::Terminal { .. }) | None
                    | Some(RetainedLaunchOutcome::Published) => return launch_ready(existing),
                }
            } else {
                let cancellation = Arc::new(LaunchCancellation::default());
                if stopping {
                    let _ = cancellation.request(CancellationReason::SupervisorShutdown);
                }
                launches.insert(
                    key.process_id(),
                    RetainedLaunch {
                        key,
                        request_digest,
                        cancellation: Arc::clone(&cancellation),
                        outcome: None,
                    },
                );
                LaunchWork::Native(cancellation)
            }
        };
        drop(launch_boundary);
        match work {
            LaunchWork::Native(cancellation) => {
                match self.completion_status(key, Some(request_digest)) {
                    Ok(Some(_)) => {
                        self.remove_launch(key, request_digest);
                        return Ok(true);
                    }
                    Ok(None) => {}
                    Err(error) => {
                        self.record_launch_outcome(
                            key,
                            request_digest,
                            RetainedLaunchOutcome::Failed(RetainedFailure::capture(&error)),
                        );
                        return Err(error);
                    }
                }
                self.spawn_launch_worker(key, request, request_digest, cancellation)
            }
            LaunchWork::Completion(terminal) => {
                self.spawn_terminal_completion_retry(key, request_digest, terminal)
            }
        }
    }

    fn spawn_launch_worker(
        self: &Arc<Self>,
        key: RetainedProcessKey,
        request: RetainedOwnerRequest,
        request_digest: peritus_types::Sha256Digest,
        cancellation: Arc<LaunchCancellation>,
    ) -> Result<bool, ProcessError> {
        let worker_owner = Arc::clone(self);
        let worker = thread::Builder::new()
            .name("peritus-retained-launch".to_owned())
            .spawn(move || {
                let outcome = catch_unwind(AssertUnwindSafe(|| {
                    worker_owner.launch_exact(
                        key,
                        request,
                        request_digest,
                        Arc::clone(&cancellation),
                    )
                }))
                .unwrap_or_else(|_| {
                    Err(owner_error(
                        ErrorCode::Supervisor,
                        RecoveryClass::ReopenAndReconcile,
                        "retained launch worker panicked",
                    ))
                });
                match outcome {
                    Ok(LaunchCompletion::Published) => worker_owner.record_launch_outcome(
                        key,
                        request_digest,
                        RetainedLaunchOutcome::Published,
                    ),
                    Ok(LaunchCompletion::Terminal(terminal)) => {
                        worker_owner.publish_terminal_launch(key, request_digest, terminal);
                    }
                    Err(error) => worker_owner.record_launch_outcome(
                        key,
                        request_digest,
                        RetainedLaunchOutcome::Failed(RetainedFailure::capture(&error)),
                    ),
                }
            });
        match worker {
            Ok(worker) => {
                self.launch_workers
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push(worker);
                Ok(false)
            }
            Err(_) => {
                let error = owner_error(
                    ErrorCode::Supervisor,
                    RecoveryClass::ReopenAndReconcile,
                    "retained launch worker cannot be created",
                );
                self.record_launch_outcome(
                    key,
                    request_digest,
                    RetainedLaunchOutcome::Failed(RetainedFailure::capture(&error)),
                );
                Err(error)
            }
        }
    }

    pub(super) fn launch_status(
        &self,
        key: RetainedProcessKey,
        request_digest: peritus_types::Sha256Digest,
    ) -> Result<bool, ProcessError> {
        self.reap_launch_workers();
        self.reap_completion_workers();
        {
            let launches = self
                .launches
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(launch) = launches.get(&key.process_id()) {
                if launch.key != key || launch.request_digest != request_digest {
                    return Err(owner_error(
                        ErrorCode::AuthorizationMismatch,
                        RecoveryClass::Quarantine,
                        "retained launch status binding differs",
                    ));
                }
                return launch_ready(launch);
            }
        }
        if self
            .completion_status(key, Some(request_digest))?
            .is_some()
        {
            Ok(true)
        } else {
            Err(owner_error(
                ErrorCode::Indeterminate,
                RecoveryClass::ReopenAndReconcile,
                "retained launch status is unavailable",
            ))
        }
    }

    fn launch_exact(
        self: &Arc<Self>,
        key: RetainedProcessKey,
        request: RetainedOwnerRequest,
        request_digest: peritus_types::Sha256Digest,
        cancellation: Arc<LaunchCancellation>,
    ) -> Result<LaunchCompletion, ProcessError> {
        let store = self.process_store()?;
        let reservation = store
            .retained_owner_reservation(key.process_id())?
            .ok_or_else(|| owner_error(
                ErrorCode::CorruptRecovery,
                RecoveryClass::Quarantine,
                "retained owner consumption claim is missing",
            ))?;
        if reservation.operation_digest() != key.operation_digest()
            || reservation.request_digest() != request_digest
            || reservation.request() != request.encode()
        {
            return Err(owner_error(
                ErrorCode::CorruptRecovery,
                RecoveryClass::Quarantine,
                "retained owner registry binding differs from the request",
            ));
        }
        if reservation.phase() != LifecyclePhase::Authorized {
            return terminal_after_launch_error(
                &store,
                key,
                request.execution_plan().digest(),
                owner_error(
                    ErrorCode::Indeterminate,
                    RecoveryClass::ReopenAndReconcile,
                    "retained launch was already intended and cannot be redispatched",
                ),
            );
        }
        let gateway = ExecutionGateway::new(store.clone());
        if !self.accepting.load(Ordering::Acquire) {
            let _ = cancellation.request(CancellationReason::SupervisorShutdown);
        }
        if let Some(reason) = cancellation.reason() {
            return gateway
                .cancel_retained_before_preparation(&request, reason)
                .map(LaunchCompletion::Terminal)
                .or_else(|error| {
                    terminal_after_launch_error(
                        &store,
                        key,
                        request.execution_plan().digest(),
                        error,
                    )
                });
        }
        let active = Arc::clone(&self.accepting);
        let probe_cancellation = Arc::clone(&cancellation);
        let backend = match reconstruct_backend(
            request.backend_factory_request(),
            &self.config,
            request.execution_plan(),
            request.sandbox_plan(),
            request.backend_descriptor_digest(),
            request.backend_support_digest(),
            request.backend_preparation_digest(),
            move || {
                if !active.load(Ordering::Acquire) {
                    let _ = probe_cancellation.request(CancellationReason::SupervisorShutdown);
                }
                probe_cancellation.reason().is_none()
            },
        ) {
            Ok(backend) => backend,
            Err(error) => {
                if let Some(reason) = cancellation.reason() {
                    return gateway
                        .cancel_retained_before_preparation(&request, reason)
                        .map(LaunchCompletion::Terminal)
                        .or_else(|error| {
                            terminal_after_launch_error(
                                &store,
                                key,
                                request.execution_plan().digest(),
                                error,
                            )
                        });
                }
                return Err(error);
            }
        };
        let descriptor = backend.descriptor().clone();
        if descriptor.digest() != request.backend_descriptor_digest()
            || descriptor.support_digest() != request.backend_support_digest()
        {
            return Err(owner_error(
                ErrorCode::PlanMismatch,
                RecoveryClass::Quarantine,
                "reconstructed retained backend descriptor differs",
            ));
        }
        let admission = BackendAdmission::restore_exact(
            request.sandbox_plan(),
            descriptor,
            request.backend_preparation_digest(),
        )
        .map_err(|_| {
            owner_error(
                ErrorCode::PlanMismatch,
                RecoveryClass::Quarantine,
                "restored retained backend admission differs",
            )
        })?;
        if let Some(reason) = cancellation.reason() {
            return gateway
                .cancel_retained_before_preparation(&request, reason)
                .map(LaunchCompletion::Terminal)
                .or_else(|error| {
                    terminal_after_launch_error(
                        &store,
                        key,
                        request.execution_plan().digest(),
                        error,
                    )
                });
        }
        let preparation_cancellation = Arc::clone(&cancellation);
        let dispatch_cancellation = Arc::clone(&cancellation);
        let owner = match gateway.resume_retained_with_backend_cancellable(
            &request,
            &admission,
            backend,
            move || preparation_cancellation.reason(),
            || {
                let guard = self
                    .launch_boundary
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if !self.accepting.load(Ordering::Acquire) {
                    let _ = dispatch_cancellation
                        .request(CancellationReason::SupervisorShutdown);
                }
                if dispatch_cancellation.reason().is_some() {
                    return Err(owner_error(
                        ErrorCode::Supervisor,
                        RecoveryClass::Terminal,
                        "retained service owner stopped before native dispatch",
                    ));
                }
                Ok(guard)
            },
        ) {
            Ok(owner) => owner,
            Err(error) => {
                return terminal_after_launch_error(
                    &store,
                    key,
                    request.execution_plan().digest(),
                    error,
                );
            }
        };
        let control = owner.control();
        let windows_owner_identity = control
            .native_recovery()
            .and_then(|recovery| recovery.windows_owner_identity());
        if !self.accepting.load(Ordering::Acquire) {
            let _ = cancellation.request(CancellationReason::SupervisorShutdown);
        }
        let _ = cancellation.publish(control.clone());
        let mut executions = self
            .executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if executions.contains_key(&key.process_id()) {
            let _ = control.cancel(CancellationReason::SupervisorShutdown);
            return Err(owner_error(
                ErrorCode::ReceiptReused,
                RecoveryClass::Quarantine,
                "retained process identity was published concurrently",
            ));
        }
        executions.insert(
            key.process_id(),
            RetainedExecution {
                key,
                request_digest,
                control: control.clone(),
                terminal: None,
                failure: None,
                publication_failure: None,
                completion_retrying: false,
                stream_snapshots: std::array::from_fn(|_| None),
                snapshot_captures: [false; 3],
                windows_owner_identity,
            },
        );
        drop(executions);
        self.spawn_completion_collector(key, request_digest, owner, control);
        Ok(LaunchCompletion::Published)
    }

    fn process_store(&self) -> Result<ProcessStore, ProcessError> {
        loop {
            let mut shared = self
                .store
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(store) = shared.value.as_ref() {
                return Ok(store.clone());
            }
            if shared.opening {
                drop(
                    self.store_changed
                        .wait(shared)
                        .unwrap_or_else(std::sync::PoisonError::into_inner),
                );
                continue;
            }
            shared.opening = true;
            break;
        }
        // This method is reached only from a launch worker. Registry initialization may inspect
        // lifetime history, but it never stalls the broker's cancellation and observation lane.
        // The initialization marker serializes concurrent openers without retaining the mutex
        // across filesystem recovery. A failed open clears the marker for a later safe retry.
        let opened = catch_unwind(AssertUnwindSafe(|| {
            #[cfg(target_os = "linux")]
            {
                let watchdog = std::env::current_exe().map_err(|_| {
                    owner_error(
                        ErrorCode::Supervisor,
                        RecoveryClass::CancelAndReap,
                        "retained process crash watchdog cannot be resolved",
                    )
                })?;
                ProcessStore::open_with_crash_watchdog(
                    self.config.paths().process_root(),
                    self.config.paths().workspace_root(),
                    watchdog,
                )
            }
            #[cfg(not(target_os = "linux"))]
            {
                ProcessStore::open(
                    self.config.paths().process_root(),
                    self.config.paths().workspace_root(),
                )
            }
        }));
        let mut shared = self
            .store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        shared.opening = false;
        let result = match opened {
            Ok(Ok(store)) => {
                shared.value = Some(store.clone());
                Ok(store)
            }
            Ok(Err(error)) => Err(error),
            Err(payload) => {
                self.store_changed.notify_all();
                drop(shared);
                std::panic::resume_unwind(payload);
            }
        };
        self.store_changed.notify_all();
        result
    }

    fn completion_status(
        &self,
        key: RetainedProcessKey,
        request_digest: Option<peritus_types::Sha256Digest>,
    ) -> Result<Option<RetainedOwnerCompletionStatus>, ProcessError> {
        let store = self
            .store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .value
            .clone();
        store.map_or(Ok(None), |store| {
            store.retained_owner_completion_status(key, self.service_owner, request_digest)
        })
    }

    fn retry_or_attach_execution(
        self: &Arc<Self>,
        key: RetainedProcessKey,
        request_digest: peritus_types::Sha256Digest,
    ) -> Result<Option<bool>, ProcessError> {
        let task = {
            let mut executions = self
                .executions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let Some(execution) = executions.get_mut(&key.process_id()) else {
                return Ok(None);
            };
            if execution.key != key || execution.request_digest != request_digest {
                return Err(owner_error(
                    ErrorCode::ReceiptReused,
                    RecoveryClass::Quarantine,
                    "retained process identity is already owned by another operation",
                ));
            }
            if let Some(observed) = execution
                .control
                .native_recovery()
                .and_then(|recovery| recovery.windows_owner_identity())
            {
                match execution.windows_owner_identity {
                    Some(expected) if expected != observed => {
                        return Err(owner_error(
                            ErrorCode::CorruptRecovery,
                            RecoveryClass::Quarantine,
                            "retained Windows owner identity changed before attachment",
                        ));
                    }
                    None => execution.windows_owner_identity = Some(observed),
                    Some(_) => {}
                }
            }
            if let Some(failure) = execution.failure {
                return Err(failure.restore());
            }
            let Some(terminal) = execution.terminal.clone() else {
                return Ok(Some(true));
            };
            if execution.completion_retrying {
                return Ok(Some(false));
            }
            if execution.publication_failure.is_none() {
                return Ok(Some(false));
            }
            execution.completion_retrying = true;
            ExecutionCompletionTask {
                key,
                request_digest,
                terminal,
                control: execution.control.clone(),
                outstanding: execution.stream_snapshots.clone(),
            }
        };
        self.spawn_execution_completion_retry(task);
        Ok(Some(false))
    }

    fn publish_terminal_launch(
        &self,
        key: RetainedProcessKey,
        request_digest: peritus_types::Sha256Digest,
        terminal: TerminalResult,
    ) {
        let result = self.process_store().and_then(|store| {
            let binding = RetainedCompletionBinding::new(
                self.service_owner,
                key,
                request_digest,
                terminal.plan_digest(),
            );
            store.persist_retained_owner_completion(
                binding,
                &terminal,
                None,
                &std::array::from_fn(|_| None),
            )
        });
        match result {
            Ok(()) => self.remove_completed_identity(key, request_digest),
            Err(error) => self.record_launch_outcome(
                key,
                request_digest,
                RetainedLaunchOutcome::Terminal {
                    terminal,
                    publication_failure: RetainedFailure::capture(&error),
                    completion_retrying: false,
                },
            ),
        }
    }

    fn spawn_terminal_completion_retry(
        self: &Arc<Self>,
        key: RetainedProcessKey,
        request_digest: peritus_types::Sha256Digest,
        terminal: TerminalResult,
    ) -> Result<bool, ProcessError> {
        let worker_owner = Arc::clone(self);
        match thread::Builder::new()
            .name("peritus-retained-completion".to_owned())
            .spawn(move || worker_owner.publish_terminal_launch(key, request_digest, terminal))
        {
            Ok(worker) => {
                self.launch_workers
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push(worker);
                Ok(false)
            }
            Err(_) => {
                let error = owner_error(
                    ErrorCode::Supervisor,
                    RecoveryClass::ReopenAndReconcile,
                    "retained completion retry worker cannot be created",
                );
                let mut launches = self
                    .launches
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if let Some(RetainedLaunch {
                    outcome: Some(RetainedLaunchOutcome::Terminal {
                        publication_failure,
                        completion_retrying,
                        ..
                    }),
                    ..
                }) = launches.get_mut(&key.process_id())
                {
                    *publication_failure = RetainedFailure::capture(&error);
                    *completion_retrying = false;
                }
                Err(error)
            }
        }
    }

    fn spawn_completion_collector(
        self: &Arc<Self>,
        key: RetainedProcessKey,
        request_digest: peritus_types::Sha256Digest,
        owner: OwnedProcess,
        control: ProcessControl,
    ) {
        let worker_owner = Arc::clone(self);
        let collector_control = control.clone();
        let worker = thread::Builder::new()
            .name("peritus-retained-collector".to_owned())
            .spawn(move || {
                let result = catch_unwind(AssertUnwindSafe(|| owner.wait())).unwrap_or_else(|_| {
                    Err(owner_error(
                        ErrorCode::Supervisor,
                        RecoveryClass::ReopenAndReconcile,
                        "retained completion collector panicked",
                    ))
                });
                match result {
                    Ok(terminal) => {
                        if let Some(task) = worker_owner.prepare_execution_completion(
                            key,
                            request_digest,
                            terminal,
                        ) {
                            worker_owner.persist_execution_completion(task);
                        }
                    }
                    Err(error) => {
                        worker_owner.record_execution_failure(key, request_digest, &error);
                    }
                }
            });
        match worker {
            Ok(worker) => self
                .completion_workers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(worker),
            Err(_) => {
                // Dropping the unstarted closure drops `owner`, which cancels and joins its
                // local supervisor. Recover the resulting terminal snapshot when available.
                if let Some(terminal) = collector_control.terminal_result() {
                    if let Some(task) = self.prepare_execution_completion(
                        key,
                        request_digest,
                        terminal,
                    ) {
                        self.persist_execution_completion(task);
                    }
                } else {
                    let error = owner_error(
                        ErrorCode::Supervisor,
                        RecoveryClass::ReopenAndReconcile,
                        "retained completion collector cannot be created",
                    );
                    self.record_execution_failure(key, request_digest, &error);
                }
            }
        }
    }

    fn prepare_execution_completion(
        &self,
        key: RetainedProcessKey,
        request_digest: peritus_types::Sha256Digest,
        terminal: TerminalResult,
    ) -> Option<ExecutionCompletionTask> {
        let mut executions = self
            .executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        loop {
            let execution = executions.get_mut(&key.process_id())?;
            if execution.key != key || execution.request_digest != request_digest {
                return None;
            }
            if execution.snapshot_captures.iter().any(|capturing| *capturing) {
                executions = self
                    .execution_changed
                    .wait(executions)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                continue;
            }
            execution.terminal = Some(terminal.clone());
            execution.failure = None;
            execution.publication_failure = None;
            execution.completion_retrying = true;
            return Some(ExecutionCompletionTask {
                key,
                request_digest,
                terminal,
                control: execution.control.clone(),
                outstanding: execution.stream_snapshots.clone(),
            });
        }
    }

    fn persist_execution_completion(&self, task: ExecutionCompletionTask) {
        let result = self.process_store().and_then(|store| {
            let binding = RetainedCompletionBinding::new(
                self.service_owner,
                task.key,
                task.request_digest,
                task.terminal.plan_digest(),
            );
            store.persist_retained_owner_completion(
                binding,
                &task.terminal,
                Some(&task.control),
                &task.outstanding,
            )
        });
        match result {
            Ok(()) => self.remove_completed_identity(task.key, task.request_digest),
            Err(error) => {
                let mut executions = self
                    .executions
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if let Ok(execution) = exact_execution_mut(&mut executions, task.key)
                    && execution.request_digest == task.request_digest
                {
                    execution.publication_failure = Some(RetainedFailure::capture(&error));
                    execution.completion_retrying = false;
                }
                self.execution_changed.notify_all();
            }
        }
    }

    fn spawn_execution_completion_retry(self: &Arc<Self>, task: ExecutionCompletionTask) {
        let key = task.key;
        let request_digest = task.request_digest;
        let worker_owner = Arc::clone(self);
        match thread::Builder::new()
            .name("peritus-retained-collector".to_owned())
            .spawn(move || worker_owner.persist_execution_completion(task))
        {
            Ok(worker) => self
                .completion_workers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(worker),
            Err(_) => {
                let error = owner_error(
                    ErrorCode::Supervisor,
                    RecoveryClass::ReopenAndReconcile,
                    "retained completion retry collector cannot be created",
                );
                let mut executions = self
                    .executions
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if let Ok(execution) = exact_execution_mut(&mut executions, key)
                    && execution.request_digest == request_digest
                {
                    execution.publication_failure = Some(RetainedFailure::capture(&error));
                    execution.completion_retrying = false;
                }
                self.execution_changed.notify_all();
            }
        }
    }

    fn record_execution_failure(
        &self,
        key: RetainedProcessKey,
        request_digest: peritus_types::Sha256Digest,
        error: &ProcessError,
    ) {
        let mut executions = self
            .executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Ok(execution) = exact_execution_mut(&mut executions, key)
            && execution.request_digest == request_digest
        {
            execution.failure = Some(RetainedFailure::capture(error));
            execution.completion_retrying = false;
        }
        self.execution_changed.notify_all();
    }

    fn remove_completed_identity(
        &self,
        key: RetainedProcessKey,
        request_digest: peritus_types::Sha256Digest,
    ) {
        let _boundary = self
            .launch_boundary
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        {
            let mut executions = self
                .executions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if executions.get(&key.process_id()).is_some_and(|execution| {
                execution.key == key && execution.request_digest == request_digest
            }) {
                executions.remove(&key.process_id());
            }
        }
        self.remove_launch_locked(key, request_digest);
        self.execution_changed.notify_all();
    }

    fn remove_launch(
        &self,
        key: RetainedProcessKey,
        request_digest: peritus_types::Sha256Digest,
    ) {
        let _boundary = self
            .launch_boundary
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.remove_launch_locked(key, request_digest);
    }

    fn remove_launch_locked(
        &self,
        key: RetainedProcessKey,
        request_digest: peritus_types::Sha256Digest,
    ) {
        let mut launches = self
            .launches
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if launches.get(&key.process_id()).is_some_and(|launch| {
            launch.key == key && launch.request_digest == request_digest
        }) {
            launches.remove(&key.process_id());
        }
    }

    fn record_launch_outcome(
        &self,
        key: RetainedProcessKey,
        request_digest: peritus_types::Sha256Digest,
        outcome: RetainedLaunchOutcome,
    ) {
        let mut launches = self
            .launches
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(launch) = launches.get_mut(&key.process_id())
            && launch.key == key
            && launch.request_digest == request_digest
        {
            launch.outcome = Some(outcome);
        }
    }

    fn reap_launch_workers(&self) {
        let finished = {
            let mut workers = self
                .launch_workers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut finished = Vec::new();
            let mut index = 0;
            while index < workers.len() {
                if workers[index].is_finished() {
                    finished.push(workers.swap_remove(index));
                } else {
                    index += 1;
                }
            }
            finished
        };
        for worker in finished {
            let _ = worker.join();
        }
    }

    fn reap_completion_workers(&self) {
        let finished = {
            let mut workers = self
                .completion_workers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut finished = Vec::new();
            let mut index = 0;
            while index < workers.len() {
                if workers[index].is_finished() {
                    finished.push(workers.swap_remove(index));
                } else {
                    index += 1;
                }
            }
            finished
        };
        for worker in finished {
            let _ = worker.join();
        }
    }

    pub(super) fn write_stdin(
        &self,
        key: RetainedProcessKey,
        bytes: Vec<u8>,
        _wait: bool,
    ) -> Result<(), ProcessError> {
        // The independently retained broker must never block on daemon-owned patience. It returns
        // typed backpressure so the client can wait and retry only a proven-unadmitted command.
        self.with_control(key, |control| control.write_stdin(bytes))
    }

    pub(super) fn close_stdin(
        &self,
        key: RetainedProcessKey,
        _wait: bool,
    ) -> Result<(), ProcessError> {
        self.with_control(key, ProcessControl::close_stdin)
    }

    pub(super) fn resize(
        &self,
        key: RetainedProcessKey,
        size: TerminalSize,
    ) -> Result<(), ProcessError> {
        self.with_control(key, |control| control.resize(size))
    }

    pub(super) fn signal(
        &self,
        key: RetainedProcessKey,
        signal: ProcessSignal,
    ) -> Result<(), ProcessError> {
        self.with_control(key, |control| control.signal(signal))
    }

    pub(super) fn cancel(
        &self,
        key: RetainedProcessKey,
        request_digest: peritus_types::Sha256Digest,
        reason: CancellationReason,
    ) -> Result<(), ProcessError> {
        // Serialize cancellation admission with the exact native-dispatch boundary. Whichever
        // side acquires this fence first is the linearized winner; a post-dispatch cancellation
        // is retained until the newly created control is published below.
        let dispatch = self
            .launch_boundary
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let cancellation = {
            let launches = self
                .launches
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let Some(launch) = launches.get(&key.process_id()) else {
                drop(launches);
                drop(dispatch);
                return if self
                    .completion_status(key, Some(request_digest))?
                    .is_some()
                {
                    Err(owner_closed())
                } else {
                    Err(owner_error(
                        ErrorCode::Indeterminate,
                        RecoveryClass::ReopenAndReconcile,
                        "retained launch is unavailable",
                    ))
                };
            };
            if launch.key != key || launch.request_digest != request_digest {
                return Err(owner_error(
                    ErrorCode::AuthorizationMismatch,
                    RecoveryClass::Quarantine,
                    "retained launch binding differs",
                ));
            }
            match launch.outcome.as_ref() {
                Some(RetainedLaunchOutcome::Terminal { publication_failure, .. }) => {
                    return Err(publication_failure.restore());
                }
                Some(RetainedLaunchOutcome::Failed(failure)) => {
                    return Err(failure.restore());
                }
                None | Some(RetainedLaunchOutcome::Published) => {
                    Arc::clone(&launch.cancellation)
                }
            }
        };
        let result = cancellation.request(reason);
        drop(dispatch);
        result
    }

    pub(super) fn observe(
        &self,
        key: RetainedProcessKey,
        cursor: ProcessCursor,
        max_events: usize,
        _wait: Option<Duration>,
    ) -> Result<RetainedOwnerObservation, ProcessError> {
        let control = {
            let executions = self
                .executions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match executions.get(&key.process_id()) {
                None => None,
                Some(execution) if execution.key != key => {
                    return Err(owner_error(
                        ErrorCode::AuthorizationMismatch,
                        RecoveryClass::Quarantine,
                        "retained process control key differs",
                    ));
                }
                Some(execution) => {
                    if let Some(failure) = execution.failure {
                        return Err(failure.restore());
                    }
                    if execution.terminal.is_some()
                        && !execution.completion_retrying
                        && let Some(failure) = execution.publication_failure
                    {
                        return Err(failure.restore());
                    }
                    Some(execution.control.clone())
                }
            }
        };
        if let Some(control) = control {
            let events = control.read_events(cursor, max_events.min(RETAINED_EVENT_PAGE));
            let native_recovery = control.native_recovery();
            if let Some(observed) = native_recovery
                .as_ref()
                .and_then(peritus_process::NativeSessionRecovery::windows_owner_identity)
            {
                let mut executions = self
                    .executions
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let execution = exact_execution_mut(&mut executions, key)?;
                match execution.windows_owner_identity {
                    Some(expected) if expected != observed => {
                        return Err(owner_error(
                            ErrorCode::CorruptRecovery,
                            RecoveryClass::Quarantine,
                            "retained Windows owner identity changed across attachment",
                        ));
                    }
                    None => execution.windows_owner_identity = Some(observed),
                    Some(_) => {}
                }
            }
            // Terminal visibility belongs to the durable receipt handoff. While this live entry
            // remains, its collector is still joining or publishing and ownership is unfinished.
            return Ok(RetainedOwnerObservation::new(
                events,
                Vec::new(),
                Vec::new(),
                Vec::new(),
                None,
                false,
                control.tree_identity(),
                native_recovery,
            ));
        }
        let store = self
            .store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .value
            .clone();
        if let Some(observation) = store
            .map(|store| {
                store.retained_owner_completion_observe(
                    key,
                    self.service_owner,
                    cursor,
                    max_events.min(RETAINED_EVENT_PAGE),
                )
            })
            .transpose()?
            .flatten()
        {
            return Ok(observation);
        }
        if self.launch_pending(key)? {
            Ok(RetainedOwnerObservation::new(
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                None,
                false,
                None,
                None,
            ))
        } else {
            Err(owner_error(
                ErrorCode::Indeterminate,
                RecoveryClass::ReopenAndReconcile,
                "retained owner observation is unavailable",
            ))
        }
    }

    pub(super) fn read_stream_page(
        &self,
        key: RetainedProcessKey,
        stream: OutputStream,
        snapshot_digest: Option<peritus_types::Sha256Digest>,
        offset: u64,
        max_bytes: usize,
    ) -> Result<RetainedStreamPage, ProcessError> {
        if max_bytes == 0 {
            return Err(owner_error(
                ErrorCode::InvalidInput,
                RecoveryClass::CorrectRequest,
                "retained stream page size is zero",
            ));
        }
        if let Some(page) = self.read_active_stream_page(
            key,
            stream,
            snapshot_digest,
            offset,
            max_bytes,
        )? {
            return Ok(page);
        }
        let store = self
            .store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .value
            .clone();
        if let Some(page) = store
            .map(|store| {
                store.retained_owner_completion_stream_page(
                    key,
                    self.service_owner,
                    stream,
                    snapshot_digest,
                    offset,
                    max_bytes,
                )
            })
            .transpose()?
            .flatten()
        {
            return Ok(page);
        }
        let _ = self.launch_pending(key)?;
        Err(owner_error(
            ErrorCode::Indeterminate,
            RecoveryClass::ReopenAndReconcile,
            "retained stream snapshot is unavailable",
        ))
    }

    fn read_active_stream_page(
        &self,
        key: RetainedProcessKey,
        stream: OutputStream,
        snapshot_digest: Option<peritus_types::Sha256Digest>,
        offset: u64,
        max_bytes: usize,
    ) -> Result<Option<RetainedStreamPage>, ProcessError> {
        let index = stream_index(stream);
        loop {
            let action = {
                let mut executions = self
                    .executions
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                loop {
                    let Some(execution) = executions.get_mut(&key.process_id()) else {
                        return Ok(None);
                    };
                    if execution.key != key {
                        return Err(owner_error(
                            ErrorCode::AuthorizationMismatch,
                            RecoveryClass::Quarantine,
                            "retained stream control key differs",
                        ));
                    }
                    if let Some(failure) = execution.failure {
                        return Err(failure.restore());
                    }
                    if let Some(snapshot) = execution.stream_snapshots[index].as_ref() {
                        match snapshot_digest {
                            Some(digest) if digest == snapshot.digest() => {
                                break SnapshotAction::Ready(snapshot.clone());
                            }
                            Some(_) if execution.terminal.is_none() => {
                                return Err(owner_error(
                                    ErrorCode::Indeterminate,
                                    RecoveryClass::ReopenAndReconcile,
                                    "retained stream snapshot changed",
                                ));
                            }
                            Some(_) | None => {}
                        }
                    }
                    if execution.terminal.is_some() {
                        break SnapshotAction::CaptureFinal(execution.control.clone());
                    }
                    if snapshot_digest.is_some() {
                        return Err(owner_error(
                            ErrorCode::Indeterminate,
                            RecoveryClass::ReopenAndReconcile,
                            "retained stream snapshot is unavailable",
                        ));
                    }
                    if execution.snapshot_captures[index] {
                        executions = self
                            .execution_changed
                            .wait(executions)
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        continue;
                    }
                    execution.snapshot_captures[index] = true;
                    break SnapshotAction::CaptureOutstanding(execution.control.clone());
                }
            };
            match action {
                SnapshotAction::Ready(snapshot) => {
                    return stream_page(&snapshot, offset, max_bytes).map(Some);
                }
                SnapshotAction::CaptureFinal(control) => {
                    let snapshot = RetainedStreamSnapshot::new(
                        stream,
                        control.retained_stream_output(stream),
                    );
                    if snapshot_digest.is_some_and(|digest| digest != snapshot.digest()) {
                        return Err(owner_error(
                            ErrorCode::Indeterminate,
                            RecoveryClass::ReopenAndReconcile,
                            "retained stream snapshot changed",
                        ));
                    }
                    return stream_page(&snapshot, offset, max_bytes).map(Some);
                }
                SnapshotAction::CaptureOutstanding(control) => {
                    let captured = RetainedStreamSnapshot::new(
                        stream,
                        control.retained_stream_output(stream),
                    );
                    let snapshot = {
                        let mut executions = self
                            .executions
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        let execution = exact_execution_mut(&mut executions, key)?;
                        execution.snapshot_captures[index] = false;
                        execution.stream_snapshots[index] = Some(captured.clone());
                        self.execution_changed.notify_all();
                        captured
                    };
                    return stream_page(&snapshot, offset, max_bytes).map(Some);
                }
            }
        }
    }

    pub(super) fn wait(
        &self,
        key: RetainedProcessKey,
    ) -> Result<Option<TerminalResult>, ProcessError> {
        {
            let executions = self
                .executions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(execution) = executions.get(&key.process_id()) {
                if execution.key != key {
                    return Err(owner_error(
                        ErrorCode::AuthorizationMismatch,
                        RecoveryClass::Quarantine,
                        "retained process owner key differs",
                    ));
                }
                if let Some(failure) = execution.failure {
                    return Err(failure.restore());
                }
                if execution.terminal.is_some()
                    && !execution.completion_retrying
                    && let Some(failure) = execution.publication_failure
                {
                    return Err(failure.restore());
                }
                return Ok(None);
            }
        }
        if let Some(status) = self.completion_status(key, None)? {
            return Ok(Some(status.terminal_result().clone()));
        }
        let _ = self.launch_pending(key)?;
        Ok(None)
    }

    fn launch_pending(&self, key: RetainedProcessKey) -> Result<bool, ProcessError> {
        let launches = self
            .launches
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let launch = launches.get(&key.process_id()).ok_or_else(|| owner_error(
            ErrorCode::Indeterminate,
            RecoveryClass::ReopenAndReconcile,
            "retained launch is unavailable",
        ))?;
        if launch.key != key {
            return Err(owner_error(
                ErrorCode::AuthorizationMismatch,
                RecoveryClass::Quarantine,
                "retained launch control key differs",
            ));
        }
        match launch.outcome.as_ref() {
            None => Ok(true),
            Some(RetainedLaunchOutcome::Failed(failure)) => Err(failure.restore()),
            Some(RetainedLaunchOutcome::Terminal { publication_failure, .. }) => {
                Err(publication_failure.restore())
            }
            Some(RetainedLaunchOutcome::Published) => Ok(false),
        }
    }

    pub(super) fn begin_shutdown(&self) {
        self.accepting.store(false, Ordering::Release);
        self.cancel_pending_launches(CancellationReason::SupervisorShutdown);
        self.cancel_owned_controls();
        let launch = self
            .launch_boundary
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        drop(launch);
        // A launch already inside the admission boundary may have inserted its slot after the
        // first scan. Cancel that slot before joining its worker. It may also have published its
        // control while shutdown crossed the dispatch boundary, so repeat both scans.
        self.cancel_pending_launches(CancellationReason::SupervisorShutdown);
        self.cancel_owned_controls();
    }

    pub(super) fn shutdown(&self) -> Vec<ProcessError> {
        self.begin_shutdown();
        let launch_workers = std::mem::take(
            &mut *self
                .launch_workers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        let mut failures = Vec::new();
        for worker in launch_workers {
            if worker.join().is_err() {
                failures.push(owner_error(
                    ErrorCode::Supervisor,
                    RecoveryClass::ReopenAndReconcile,
                    "retained launch worker panicked during shutdown",
                ));
            }
        }
        // Launch workers are the only creators of first-owner collectors. Once all launch
        // workers have joined, this second cancellation reaches every published control.
        self.cancel_owned_controls();
        let completion_workers = std::mem::take(
            &mut *self
                .completion_workers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        for worker in completion_workers {
            if worker.join().is_err() {
                failures.push(owner_error(
                    ErrorCode::Supervisor,
                    RecoveryClass::ReopenAndReconcile,
                    "retained completion collector panicked during shutdown",
                ));
            }
        }
        failures.extend(
            self.executions
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .values()
                .filter_map(|execution| execution.failure.or(execution.publication_failure))
                .map(RetainedFailure::restore),
        );
        failures.extend(
            self.launches
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .values()
                .filter_map(|launch| match launch.outcome.as_ref() {
                    Some(RetainedLaunchOutcome::Terminal { publication_failure, .. }) => {
                        Some(publication_failure.restore())
                    }
                    None | Some(RetainedLaunchOutcome::Published)
                    | Some(RetainedLaunchOutcome::Failed(_)) => None,
                }),
        );
        failures
    }

    fn cancel_owned_controls(&self) {
        let controls = self
            .executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .values()
            .filter(|execution| execution.terminal.is_none())
            .map(|execution| execution.control.clone())
            .collect::<Vec<_>>();
        for control in controls {
            let _ = control.cancel(CancellationReason::SupervisorShutdown);
        }
    }

    fn cancel_pending_launches(&self, reason: CancellationReason) {
        let cancellations = self
            .launches
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .values()
            .filter(|launch| {
                !matches!(
                    launch.outcome.as_ref(),
                    Some(RetainedLaunchOutcome::Terminal { .. })
                        | Some(RetainedLaunchOutcome::Failed(_))
                )
            })
            .map(|launch| Arc::clone(&launch.cancellation))
            .collect::<Vec<_>>();
        for cancellation in cancellations {
            let _ = cancellation.request(reason);
        }
    }

    pub(super) fn serve(
        self: &Arc<Self>,
        reader: &mut impl std::io::Read,
        writer: &mut impl std::io::Write,
    ) -> std::io::Result<()> {
        while let Some(command) = protocol::read_command(reader)? {
            let reply = match command {
                protocol::Command::Launch { key, request_digest, request } => self
                    .launch_or_attach(key, request, request_digest)
                    .map(|ready| {
                        if ready { protocol::Reply::Unit } else { protocol::Reply::Pending }
                    }),
                protocol::Command::LaunchStatus { key, request_digest } => self
                    .launch_status(key, request_digest)
                    .map(|ready| {
                        if ready { protocol::Reply::Unit } else { protocol::Reply::Pending }
                    }),
                protocol::Command::Write { key, wait, bytes } => self
                    .write_stdin(key, bytes, wait)
                    .map(|()| protocol::Reply::Unit),
                protocol::Command::Close { key, wait } => self
                    .close_stdin(key, wait)
                    .map(|()| protocol::Reply::Unit),
                protocol::Command::Resize { key, size } => self
                    .resize(key, size)
                    .map(|()| protocol::Reply::Unit),
                protocol::Command::Signal { key, signal } => self
                    .signal(key, signal)
                    .map(|()| protocol::Reply::Unit),
                protocol::Command::Cancel { key, request_digest, reason } => self
                    .cancel(key, request_digest, reason)
                    .map(|()| protocol::Reply::Unit),
                protocol::Command::Observe { key, cursor, max_events, wait } => self
                    .observe(key, cursor, max_events, wait)
                    .map(protocol::Reply::Observation),
                protocol::Command::Stream {
                    key,
                    stream,
                    snapshot_digest,
                    offset,
                    max_bytes,
                } => self
                    .read_stream_page(key, stream, snapshot_digest, offset, max_bytes)
                    .map(protocol::Reply::StreamPage),
                protocol::Command::Wait { key } => self.wait(key).map(|terminal| {
                    terminal.map_or(protocol::Reply::Pending, protocol::Reply::Terminal)
                }),
            }
            .unwrap_or_else(protocol::Reply::Error);
            protocol::write_reply(writer, &reply)?;
        }
        Ok(())
    }

    fn control(&self, key: RetainedProcessKey) -> Result<ProcessControl, ProcessError> {
        let executions = self
            .executions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        exact_execution(&executions, key).map(|execution| execution.control.clone())
    }

    fn with_control<T>(
        &self,
        key: RetainedProcessKey,
        apply: impl FnOnce(&ProcessControl) -> Result<T, ProcessError>,
    ) -> Result<T, ProcessError> {
        match self.control(key) {
            Ok(control) => apply(&control),
            Err(error) if error.code() == ErrorCode::Indeterminate => {
                if self.completion_status(key, None)?.is_some() {
                    Err(owner_closed())
                } else {
                    Err(error)
                }
            }
            Err(error) => Err(error),
        }
    }
}

const fn stream_index(stream: OutputStream) -> usize {
    match stream {
        OutputStream::Stdout => 0,
        OutputStream::Stderr => 1,
        OutputStream::Terminal => 2,
    }
}

fn launch_ready(launch: &RetainedLaunch) -> Result<bool, ProcessError> {
    match launch.outcome.as_ref() {
        None => Ok(false),
        Some(RetainedLaunchOutcome::Published) => Ok(true),
        Some(RetainedLaunchOutcome::Terminal {
            publication_failure,
            completion_retrying,
            ..
        }) => {
            if *completion_retrying { Ok(false) } else { Err(publication_failure.restore()) }
        }
        Some(RetainedLaunchOutcome::Failed(failure)) => Err(failure.restore()),
    }
}

fn exact_execution(
    executions: &BTreeMap<ProcessId, RetainedExecution>,
    key: RetainedProcessKey,
) -> Result<&RetainedExecution, ProcessError> {
    let execution = executions.get(&key.process_id()).ok_or_else(|| owner_error(
        ErrorCode::Indeterminate,
        RecoveryClass::ReopenAndReconcile,
        "retained process owner is unavailable",
    ))?;
    if execution.key != key {
        return Err(owner_error(
            ErrorCode::AuthorizationMismatch,
            RecoveryClass::Quarantine,
            "retained process control key differs",
        ));
    }
    Ok(execution)
}

fn exact_execution_mut(
    executions: &mut BTreeMap<ProcessId, RetainedExecution>,
    key: RetainedProcessKey,
) -> Result<&mut RetainedExecution, ProcessError> {
    let execution = executions.get_mut(&key.process_id()).ok_or_else(|| owner_error(
        ErrorCode::Indeterminate,
        RecoveryClass::ReopenAndReconcile,
        "retained process owner is unavailable",
    ))?;
    if execution.key != key {
        return Err(owner_error(
            ErrorCode::AuthorizationMismatch,
            RecoveryClass::Quarantine,
            "retained process control key differs",
        ));
    }
    Ok(execution)
}

fn stream_page(
    snapshot: &RetainedStreamSnapshot,
    offset: u64,
    max_bytes: usize,
) -> Result<RetainedStreamPage, ProcessError> {
    let total_bytes = u64::try_from(snapshot.bytes().len()).map_err(|_| {
        owner_error(
            ErrorCode::InvalidInput,
            RecoveryClass::ReopenAndReconcile,
            "retained stream snapshot length is unrepresentable",
        )
    })?;
    let start = usize::try_from(offset).map_err(|_| {
        owner_error(
            ErrorCode::InvalidInput,
            RecoveryClass::CorrectRequest,
            "retained stream page offset is unrepresentable",
        )
    })?;
    if start > snapshot.bytes().len() {
        return Err(owner_error(
            ErrorCode::InvalidInput,
            RecoveryClass::CorrectRequest,
            "retained stream page offset exceeds its snapshot",
        ));
    }
    let end = start
        .checked_add(max_bytes.min(RETAINED_STREAM_PAGE_BYTES))
        .unwrap_or(snapshot.bytes().len())
        .min(snapshot.bytes().len());
    RetainedStreamPage::new(
        snapshot.stream(),
        snapshot.digest(),
        total_bytes,
        offset,
        snapshot.bytes()[start..end].to_vec(),
    )
}

fn terminal_after_launch_error(
    store: &ProcessStore,
    key: RetainedProcessKey,
    plan_digest: peritus_types::Sha256Digest,
    error: ProcessError,
) -> Result<LaunchCompletion, ProcessError> {
    match store.terminal_result(key.process_id()) {
        Ok(terminal)
            if terminal.process_id() == key.process_id()
                && terminal.plan_digest() == plan_digest =>
        {
            Ok(LaunchCompletion::Terminal(terminal))
        }
        Ok(_) => Err(owner_error(
            ErrorCode::CorruptRecovery,
            RecoveryClass::Quarantine,
            "retained launch terminal binding differs",
        )),
        Err(_) => Err(error),
    }
}

const fn owner_error(
    code: ErrorCode,
    recovery: RecoveryClass,
    detail: &'static str,
) -> ProcessError {
    ProcessError::new(code, ProcessOperation::Reconcile, recovery, detail)
}

const fn owner_closed() -> ProcessError {
    ProcessError::new(
        ErrorCode::Input,
        ProcessOperation::Control,
        RecoveryClass::Terminal,
        "retained process owner has already terminated",
    )
}
