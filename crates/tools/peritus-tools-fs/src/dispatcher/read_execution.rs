use std::{
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
        mpsc::{self, Receiver, TryRecvError},
    },
    thread::{self, JoinHandle},
};

use peritus_policy::AuthorityInstant;
use peritus_tool_protocol::{
    CancellationReason, FailureCategory, PreparedToolCall, RecoveryRoute, ResponsibleSubsystem,
    ResultStatus, Retryability, ToolControl, ToolFailure, ToolResult,
};
use peritus_tool_router::{
    DispatchFailure, ExecutionUpdate, RecoveryObservation, ToolExecution, ToolStart,
};
use peritus_workspace::ReadOnlyWorkspace;
use std::time::Instant;

use super::operations::{
    bounded, compiled_operation, execute_read_cancellable, finish, protocol_failure, tool_failure,
};
use crate::{
    FsDispatchKind, FsToolError, FsToolErrorKind, FsToolOperation, RecoveryClass, RenderedOutput,
};

enum ReadCompletion {
    Ready { rendered: RenderedOutput, completed_at: AuthorityInstant },
    Cancelled(ResultStatus),
    Failed(FsToolError),
}

struct FsReadExecution {
    prepared: PreparedToolCall,
    started_at: AuthorityInstant,
    receiver: Receiver<ReadCompletion>,
    worker: Option<JoinHandle<()>>,
    phase: Arc<AtomicU8>,
    terminal: Option<ToolResult>,
}

pub(super) fn spawn_read(
    kind: FsDispatchKind,
    workspace: Arc<ReadOnlyWorkspace>,
    prepared: PreparedToolCall,
    started_at: AuthorityInstant,
    monotonic_started: Instant,
) -> Result<ToolStart, DispatchFailure> {
    let (sender, receiver) = mpsc::sync_channel(1);
    let phase = Arc::new(AtomicU8::new(PREPARED));
    let worker_phase = Arc::clone(&phase);
    let worker_prepared = prepared.clone();
    let worker = thread::Builder::new().name("peritus-fs-read".to_owned()).spawn(move || {
        match worker_phase.compare_exchange(PREPARED, APPLYING, Ordering::AcqRel, Ordering::Acquire)
        {
            Err(requested)
                if requested == CANCELLED_BEFORE_APPLY || requested == TIMED_OUT_BEFORE_APPLY =>
            {
                let status = if requested == TIMED_OUT_BEFORE_APPLY {
                    ResultStatus::TimedOut
                } else {
                    ResultStatus::Cancelled
                };
                let _ = sender.send(ReadCompletion::Cancelled(status));
            }
            Ok(_) => {
                let result = execute_read_cancellable(
                    kind,
                    &workspace,
                    worker_prepared.arguments(),
                    worker_prepared.call().limits().output_bytes(),
                    &|| {
                        matches!(
                            worker_phase.load(Ordering::Acquire),
                            CANCEL_REQUESTED | TIMED_OUT_DURING_WORK
                        )
                    },
                );
                let completion = match result {
                    Ok(Some(rendered)) => worker_completion_instant(started_at, monotonic_started)
                        .map_or_else(ReadCompletion::Failed, |completed_at| {
                            ReadCompletion::Ready { rendered, completed_at }
                        }),
                    Ok(None) => ReadCompletion::Cancelled(requested_status(
                        worker_phase.load(Ordering::Acquire),
                    )),
                    Err(error) => ReadCompletion::Failed(error),
                };
                worker_phase.store(FINISHED, Ordering::Release);
                let _ = sender.send(completion);
            }
            Err(_) => {
                worker_phase.store(FINISHED, Ordering::Release);
                let _ = sender.send(ReadCompletion::Failed(FsToolError::new(
                    FsToolErrorKind::Inspection,
                    compiled_operation(kind),
                    RecoveryClass::Reobserve,
                    "filesystem read worker entered an invalid safe-point state",
                )));
            }
        }
        worker_phase.store(FINISHED, Ordering::Release);
    });
    let worker = worker.map_err(|_| protocol_failure("filesystem read worker could not start"))?;
    Ok(ToolStart::Active(Box::new(FsReadExecution {
        prepared,
        started_at,
        receiver,
        worker: Some(worker),
        phase,
        terminal: None,
    })))
}

impl FsReadExecution {
    fn poll_owned(
        &mut self,
        _observed_at: AuthorityInstant,
    ) -> Result<ExecutionUpdate, DispatchFailure> {
        if let Some(result) = &self.terminal {
            return ExecutionUpdate::new(&self.prepared, Vec::new(), Some(result.clone()))
                .map_err(|_| protocol_failure("filesystem read result could not be repeated"));
        }
        match self.receiver.try_recv() {
            Ok(ReadCompletion::Ready { rendered, completed_at }) => {
                self.join_worker()?;
                let terminal = finish(&self.prepared, &rendered, self.started_at, completed_at)?;
                self.terminal = Some(terminal.clone());
                ExecutionUpdate::new(&self.prepared, Vec::new(), Some(terminal))
                    .map_err(|_| protocol_failure("filesystem read update is invalid"))
            }
            Ok(ReadCompletion::Cancelled(status)) => {
                self.join_worker()?;
                Err(read_cancellation_failure(status))
            }
            Ok(ReadCompletion::Failed(error)) => {
                self.join_worker()?;
                Err(tool_failure(&error))
            }
            Err(TryRecvError::Empty) => ExecutionUpdate::new(&self.prepared, Vec::new(), None)
                .map_err(|_| protocol_failure("filesystem read active update is invalid")),
            Err(TryRecvError::Disconnected) => {
                Err(protocol_failure("filesystem read worker ended without an outcome"))
            }
        }
    }

    fn join_worker(&mut self) -> Result<(), DispatchFailure> {
        if self.worker.take().is_some_and(|worker| worker.join().is_err()) {
            return Err(protocol_failure(
                "filesystem read worker panicked before closing its outcome",
            ));
        }
        Ok(())
    }
}

impl ToolExecution for FsReadExecution {
    fn poll(&mut self, observed_at: AuthorityInstant) -> Result<ExecutionUpdate, DispatchFailure> {
        self.poll_owned(observed_at)
    }

    fn control(
        &mut self,
        control: ToolControl,
        observed_at: AuthorityInstant,
    ) -> Result<ExecutionUpdate, DispatchFailure> {
        if control != ToolControl::Poll {
            return Err(protocol_failure("filesystem reads support polling and cancellation only"));
        }
        self.poll_owned(observed_at)
    }

    fn cancel(
        &mut self,
        reason: CancellationReason,
        observed_at: AuthorityInstant,
    ) -> Result<ExecutionUpdate, DispatchFailure> {
        let requested = if reason == CancellationReason::Deadline {
            TIMED_OUT_BEFORE_APPLY
        } else {
            CANCELLED_BEFORE_APPLY
        };
        let _ =
            self.phase.compare_exchange(PREPARED, requested, Ordering::AcqRel, Ordering::Acquire);
        let running = if reason == CancellationReason::Deadline {
            TIMED_OUT_DURING_WORK
        } else {
            CANCEL_REQUESTED
        };
        let _ = self.phase.compare_exchange(APPLYING, running, Ordering::AcqRel, Ordering::Acquire);
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

impl Drop for FsReadExecution {
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
const TIMED_OUT_DURING_WORK: u8 = 6;

const fn requested_status(phase: u8) -> ResultStatus {
    if matches!(phase, TIMED_OUT_BEFORE_APPLY | TIMED_OUT_DURING_WORK) {
        ResultStatus::TimedOut
    } else {
        ResultStatus::Cancelled
    }
}

fn read_cancellation_failure(status: ResultStatus) -> DispatchFailure {
    let (category, code, detail) = if status == ResultStatus::TimedOut {
        (FailureCategory::Timeout, "PERITUS-FS-TIMEOUT", "filesystem read reached its deadline")
    } else {
        (
            FailureCategory::Cancelled,
            "PERITUS-FS-CANCELLED",
            "filesystem read was cancelled before completion",
        )
    };
    let failure = ToolFailure::new(
        category,
        bounded(code),
        ResponsibleSubsystem::Workspace,
        Retryability::NewAction,
        RecoveryRoute::None,
        bounded(detail),
    );
    DispatchFailure::new(status, failure).expect("filesystem read cancellation is non-success")
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
