use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicU8, Ordering},
        mpsc::{self, Receiver, TryRecvError},
    },
    thread::{self, JoinHandle},
    time::Instant,
};

use peritus_policy::AuthorityInstant;
use peritus_tool_protocol::{
    CancellationReason, PreparedToolCall, ResultStatus, ToolControl, ToolResult,
};
use peritus_tool_router::{
    DispatchFailure, ExecutionUpdate, RecoveryObservation, ToolExecution, ToolStart,
};
use peritus_workspace::{AuthorizedPatch, MutationOutcome, WorkspaceGateway};

use super::operations::{
    cancellation_failure, cancelled_failure, compiled_operation, finish, poisoned_gateway,
    protocol_failure, tool_failure, workspace_error,
};
use crate::{
    FsDispatchKind, FsToolError, FsToolErrorKind, FsToolOperation, RecoveryClass, RenderedOutput,
};

enum MutationCompletion {
    Applied { outcome: MutationOutcome, completed_at: AuthorityInstant },
    Cancelled(ResultStatus),
    Failed(FsToolError),
}

struct FsMutationExecution {
    prepared: PreparedToolCall,
    started_at: AuthorityInstant,
    receiver: Receiver<MutationCompletion>,
    worker: Option<JoinHandle<()>>,
    phase: Arc<AtomicU8>,
    mutation_outcome: Arc<Mutex<Option<MutationOutcome>>>,
    terminal: Option<ToolResult>,
}

pub(super) fn spawn_mutation(
    kind: FsDispatchKind,
    gateway: &Arc<Mutex<WorkspaceGateway>>,
    mutation_outcome: Arc<Mutex<Option<MutationOutcome>>>,
    operation: AuthorizedPatch,
    prepared: PreparedToolCall,
    started_at: AuthorityInstant,
    monotonic_started: Instant,
) -> Result<ToolStart, DispatchFailure> {
    let (sender, receiver) = mpsc::sync_channel(1);
    let phase = Arc::new(AtomicU8::new(PREPARED));
    let shared_operation = Arc::new(Mutex::new(Some(operation)));
    let worker = MutationWorker {
        kind,
        phase: Arc::clone(&phase),
        gateway: Arc::clone(gateway),
        operation: Arc::clone(&shared_operation),
        started_at,
        monotonic_started,
    };
    let spawned = thread::Builder::new()
        .name("peritus-fs-mutation".to_owned())
        .spawn(move || worker.run(&sender));
    let Ok(thread) = spawned else {
        return cancel_failed_spawn(kind, gateway, &shared_operation);
    };
    Ok(ToolStart::Active(Box::new(FsMutationExecution {
        prepared,
        started_at,
        receiver,
        worker: Some(thread),
        phase,
        mutation_outcome,
        terminal: None,
    })))
}

struct MutationWorker {
    kind: FsDispatchKind,
    phase: Arc<AtomicU8>,
    gateway: Arc<Mutex<WorkspaceGateway>>,
    operation: Arc<Mutex<Option<AuthorizedPatch>>>,
    started_at: AuthorityInstant,
    monotonic_started: Instant,
}

impl MutationWorker {
    fn run(self, sender: &mpsc::SyncSender<MutationCompletion>) {
        let operation = self.operation.lock().ok().and_then(|mut operation| operation.take());
        let Some(operation) = operation else {
            let _ = sender.send(MutationCompletion::Failed(FsToolError::new(
                FsToolErrorKind::Workspace,
                compiled_operation(self.kind),
                RecoveryClass::Reconcile,
                "prepared mutation token could not be transferred to its worker",
            )));
            return;
        };
        let completion = match self.phase.compare_exchange(
            PREPARED,
            APPLYING,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Err(requested)
                if requested == CANCELLED_BEFORE_APPLY || requested == TIMED_OUT_BEFORE_APPLY =>
            {
                let result = self.gateway.lock().map_err(|_| poisoned_gateway(self.kind)).and_then(
                    |mut gateway| {
                        gateway
                            .cancel_prepared_patch(operation)
                            .map_err(|error| workspace_error(self.kind, &error))
                    },
                );
                match result {
                    Ok(()) => MutationCompletion::Cancelled(requested_status(requested)),
                    Err(error) => MutationCompletion::Failed(error),
                }
            }
            Ok(_) => {
                let result = self.gateway.lock().map_err(|_| poisoned_gateway(self.kind)).and_then(
                    |mut gateway| {
                        gateway
                            .apply_prepared_patch_cancellable(operation, || {
                                matches!(
                                    self.phase.load(Ordering::Acquire),
                                    CANCEL_REQUESTED | TIMED_OUT_DURING_APPLY
                                )
                            })
                            .map_err(|error| workspace_error(self.kind, &error))
                    },
                );
                match result {
                    Ok(Some(outcome)) => {
                        worker_completion_instant(self.started_at, self.monotonic_started)
                            .map_or_else(MutationCompletion::Failed, |completed_at| {
                                MutationCompletion::Applied { outcome, completed_at }
                            })
                    }
                    Ok(None) => MutationCompletion::Cancelled(requested_status(
                        self.phase.load(Ordering::Acquire),
                    )),
                    Err(error) => MutationCompletion::Failed(error),
                }
            }
            Err(_) => MutationCompletion::Failed(FsToolError::new(
                FsToolErrorKind::Workspace,
                compiled_operation(self.kind),
                RecoveryClass::Reconcile,
                "mutation worker entered an invalid safe-point state",
            )),
        };
        self.phase.store(FINISHED, Ordering::Release);
        let _ = sender.send(completion);
    }
}

fn cancel_failed_spawn(
    kind: FsDispatchKind,
    gateway: &Arc<Mutex<WorkspaceGateway>>,
    shared_operation: &Arc<Mutex<Option<AuthorizedPatch>>>,
) -> Result<ToolStart, DispatchFailure> {
    let cancellation = shared_operation
        .lock()
        .ok()
        .and_then(|mut operation| operation.take())
        .ok_or_else(|| {
            FsToolError::new(
                FsToolErrorKind::Workspace,
                compiled_operation(kind),
                RecoveryClass::Reconcile,
                "prepared mutation token was lost before its worker started",
            )
        })
        .and_then(|operation| {
            gateway.lock().map_err(|_| poisoned_gateway(kind)).and_then(|mut gateway| {
                gateway
                    .cancel_prepared_patch(operation)
                    .map_err(|error| workspace_error(kind, &error))
            })
        });
    match cancellation {
        Ok(()) => Err(cancelled_failure()),
        Err(error) => Err(tool_failure(&error)),
    }
}

pub(super) fn spawn_recovered_mutation(
    mutation_outcome: Arc<Mutex<Option<MutationOutcome>>>,
    outcome: MutationOutcome,
    prepared: PreparedToolCall,
    started_at: AuthorityInstant,
    monotonic_started: Instant,
) -> Result<ToolStart, DispatchFailure> {
    let (sender, receiver) = mpsc::sync_channel(1);
    let thread = thread::Builder::new()
        .name("peritus-fs-recovered-outcome".to_owned())
        .spawn(move || {
            let completion = worker_completion_instant(started_at, monotonic_started)
                .map_or_else(MutationCompletion::Failed, |completed_at| {
                    MutationCompletion::Applied { outcome, completed_at }
                });
            let _ = sender.send(completion);
        })
        .map_err(|_| protocol_failure("recovered filesystem outcome worker could not start"))?;
    Ok(ToolStart::Active(Box::new(FsMutationExecution {
        prepared,
        started_at,
        receiver,
        worker: Some(thread),
        phase: Arc::new(AtomicU8::new(FINISHED)),
        mutation_outcome,
        terminal: None,
    })))
}

impl FsMutationExecution {
    fn poll_owned(
        &mut self,
        _observed_at: AuthorityInstant,
    ) -> Result<ExecutionUpdate, DispatchFailure> {
        if let Some(result) = &self.terminal {
            return ExecutionUpdate::new(&self.prepared, Vec::new(), Some(result.clone()))
                .map_err(|_| protocol_failure("filesystem terminal result could not be repeated"));
        }
        match self.receiver.try_recv() {
            Ok(MutationCompletion::Applied { outcome, completed_at }) => {
                self.mutation_outcome
                    .lock()
                    .map_err(|_| protocol_failure("mutation outcome lock is poisoned"))?
                    .replace(outcome);
                let rendered = {
                    let outcome = self
                        .mutation_outcome
                        .lock()
                        .map_err(|_| protocol_failure("mutation outcome lock is poisoned"))?;
                    RenderedOutput::mutation(
                        outcome
                            .as_ref()
                            .ok_or_else(|| protocol_failure("mutation outcome was not retained"))?,
                    )
                    .map_err(|error| tool_failure(&error))?
                };
                self.join_worker()?;
                let terminal = finish(&self.prepared, &rendered, self.started_at, completed_at)?;
                self.terminal = Some(terminal.clone());
                ExecutionUpdate::new(&self.prepared, Vec::new(), Some(terminal))
                    .map_err(|_| protocol_failure("filesystem terminal update is invalid"))
            }
            Ok(MutationCompletion::Cancelled(status)) => {
                self.join_worker()?;
                Err(cancellation_failure(status))
            }
            Ok(MutationCompletion::Failed(error)) => {
                self.join_worker()?;
                Err(tool_failure(&error))
            }
            Err(TryRecvError::Empty) => ExecutionUpdate::new(&self.prepared, Vec::new(), None)
                .map_err(|_| protocol_failure("filesystem active update is invalid")),
            Err(TryRecvError::Disconnected) => {
                Err(protocol_failure("filesystem worker ended without an outcome"))
            }
        }
    }

    fn join_worker(&mut self) -> Result<(), DispatchFailure> {
        if self.worker.take().is_some_and(|worker| worker.join().is_err()) {
            return Err(protocol_failure("filesystem worker panicked before closing its outcome"));
        }
        Ok(())
    }
}

impl ToolExecution for FsMutationExecution {
    fn poll(&mut self, observed_at: AuthorityInstant) -> Result<ExecutionUpdate, DispatchFailure> {
        self.poll_owned(observed_at)
    }

    fn control(
        &mut self,
        control: ToolControl,
        observed_at: AuthorityInstant,
    ) -> Result<ExecutionUpdate, DispatchFailure> {
        if control != ToolControl::Poll {
            return Err(protocol_failure(
                "filesystem mutations support polling and cancellation only",
            ));
        }
        self.poll_owned(observed_at)
    }

    fn cancel(
        &mut self,
        reason: CancellationReason,
        observed_at: AuthorityInstant,
    ) -> Result<ExecutionUpdate, DispatchFailure> {
        let requested_phase = if reason == CancellationReason::Deadline {
            TIMED_OUT_BEFORE_APPLY
        } else {
            CANCELLED_BEFORE_APPLY
        };
        let _ = self.phase.compare_exchange(
            PREPARED,
            requested_phase,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        let applying_phase = if reason == CancellationReason::Deadline {
            TIMED_OUT_DURING_APPLY
        } else {
            CANCEL_REQUESTED
        };
        let _ = self.phase.compare_exchange(
            APPLYING,
            applying_phase,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        self.poll_owned(observed_at)
    }

    fn recover(
        &mut self,
        observed_at: AuthorityInstant,
    ) -> Result<RecoveryObservation, DispatchFailure> {
        let update = self.poll_owned(observed_at)?;
        if update.terminal().is_some() {
            Ok(RecoveryObservation::Completed(update))
        } else {
            Ok(RecoveryObservation::Active(update))
        }
    }
}

impl Drop for FsMutationExecution {
    fn drop(&mut self) {
        let _ = self.phase.compare_exchange(
            PREPARED,
            CANCELLED_BEFORE_APPLY,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        let _ = self.phase.compare_exchange(
            APPLYING,
            CANCEL_REQUESTED,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

const PREPARED: u8 = 0;
const CANCELLED_BEFORE_APPLY: u8 = 1;
const APPLYING: u8 = 2;
const FINISHED: u8 = 3;
const TIMED_OUT_BEFORE_APPLY: u8 = 4;
const CANCEL_REQUESTED: u8 = 5;
const TIMED_OUT_DURING_APPLY: u8 = 7;

const fn requested_status(phase: u8) -> ResultStatus {
    if matches!(phase, TIMED_OUT_BEFORE_APPLY | TIMED_OUT_DURING_APPLY) {
        ResultStatus::TimedOut
    } else {
        ResultStatus::Cancelled
    }
}

fn worker_completion_instant(
    started_at: AuthorityInstant,
    execution_started: Instant,
) -> Result<AuthorityInstant, FsToolError> {
    let elapsed = u64::try_from(execution_started.elapsed().as_millis()).map_err(|_| {
        FsToolError::new(
            FsToolErrorKind::Protocol,
            FsToolOperation::Catalog,
            RecoveryClass::Reconcile,
            "worker completion duration is not representable",
        )
    })?;
    started_at.checked_add(elapsed).map_err(|_| {
        FsToolError::new(
            FsToolErrorKind::Protocol,
            FsToolOperation::Catalog,
            RecoveryClass::Reconcile,
            "worker completion time is outside the authority clock epoch",
        )
    })
}
