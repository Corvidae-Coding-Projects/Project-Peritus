//! Isolated two-stage rollout execution contracts.

mod directive;
mod observation;
mod port;

pub use directive::{
    CandidateExecutionDirective, EvaluatorExecutionDirective, ExecutionContinuation,
    ExecutionStage, ExecutionStageIdentity,
};
pub use observation::{
    CandidateObservation, EvaluatorObservation, EvaluatorVerdict, ExecutedRollout, ExecutionFailure,
};
pub use port::{
    CancellationObservation, CancellationProbe, ExecutionStageControl, NeverCancelled,
    RolloutExecutionPort, StageCancellation, StageExecution, execute_rollout,
};
