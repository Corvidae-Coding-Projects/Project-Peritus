//! Isolated two-stage rollout execution contracts.

mod directive;
mod observation;
mod port;

pub use directive::{CandidateExecutionDirective, EvaluatorExecutionDirective};
pub use observation::{
    CandidateObservation, EvaluatorObservation, EvaluatorVerdict, ExecutedRollout, ExecutionFailure,
};
pub use port::{CancellationProbe, NeverCancelled, RolloutExecutionPort, execute_rollout};
