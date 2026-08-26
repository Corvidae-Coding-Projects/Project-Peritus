//! Bounded values retained by the debugger aggregate.

mod failure;
mod model;
mod phase;
mod records;

pub use failure::{JobFailure, JobFailureCode};
pub use model::{
    ModelAttemptFailure, ModelAttemptFailureCode, ModelAttemptObservation, ModelAttemptResult,
    ModelBudget, ModelProgress, ModelRetryPolicy, ModelWorkState,
};
pub use phase::DebuggerPhase;
pub use records::{AnalysisCounts, PublicationRecord, ReportRecord, SelectionRecord};

pub(super) fn invalid(detail: &'static str) -> crate::DebuggerError {
    crate::DebuggerError::new(
        crate::DebuggerErrorKind::InvalidInput,
        crate::DebuggerOperation::ApplyTransition,
        crate::DebuggerRecovery::CorrectInput,
        detail,
    )
}
