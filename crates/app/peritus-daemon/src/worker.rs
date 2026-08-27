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

pub use cancellation::WorkerCancellation;
pub use error::{WorkerSupervisorError, WorkerSupervisorErrorKind};
pub use limits::WorkerSupervisorLimits;
pub use observation::{
    WorkerAssignment, WorkerCancellationReason, WorkerFailureKind, WorkerTaskOutcome,
    WorkerTerminalObservation,
};
pub use snapshot::{
    WorkerCancelDisposition, WorkerCounts, WorkerDrainDisposition, WorkerReapReport,
    WorkerRemainingWork, WorkerShutdownDisposition, WorkerShutdownReport, WorkerSupervisorPhase,
    WorkerSupervisorSnapshot, WorkerTaskSnapshot, WorkerTaskState,
};
pub use supervisor::WorkerSupervisor;
