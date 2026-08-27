//! Structured bounded ownership for scheduler-dispatched Tokio worker tasks.

mod cancellation;
mod error;
mod limits;
mod observation;
mod snapshot;
mod supervisor;
mod task;

#[cfg(test)]
mod tests;

pub(crate) use cancellation::WorkerCancellation;
pub(crate) use error::{WorkerSupervisorError, WorkerSupervisorErrorKind};
pub(crate) use limits::WorkerSupervisorLimits;
pub(crate) use observation::{
    WorkerAssignment, WorkerCancellationReason, WorkerFailureKind, WorkerTaskOutcome,
    WorkerTerminalObservation,
};
pub(crate) use snapshot::{
    WorkerCancelDisposition, WorkerCounts, WorkerDrainDisposition, WorkerReapReport,
    WorkerRemainingWork, WorkerShutdownDisposition, WorkerShutdownReport, WorkerSupervisorPhase,
    WorkerSupervisorSnapshot, WorkerTaskSnapshot, WorkerTaskState,
};
pub(crate) use supervisor::WorkerSupervisor;
