//! Stable redaction-safe E3 failure vocabulary.

use core::fmt;

/// Stable evaluation failure category.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum EvaluationErrorKind {
    /// A dataset manifest is invalid.
    Manifest,
    /// A frozen profile is invalid or drifted.
    Profile,
    /// Candidate/evaluator isolation is invalid.
    Isolation,
    /// A configured or encoded bound was exceeded.
    LimitExceeded,
    /// A scheduling binding is invalid.
    Scheduling,
    /// An execution observation is invalid.
    Execution,
    /// Statistical inputs or arithmetic are invalid.
    Statistics,
    /// Required observations are incomplete.
    Incomplete,
    /// Durable state or command binding conflicts.
    Binding,
    /// A state transition is illegal.
    IllegalTransition,
    /// Stored canonical bytes are malformed or corrupt.
    Corruption,
    /// A C0 operation failed.
    Journal,
    /// Artifact storage or verification failed.
    Artifact,
    /// Evidence admission failed.
    Evidence,
    /// Recovery requires reconciliation or quarantine.
    Recovery,
}

/// Stable operation that failed.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum EvaluationOperation {
    /// Validate a dataset.
    ValidateDataset,
    /// Freeze an evaluation profile.
    FreezeProfile,
    /// Build a rollout plan.
    BuildPlan,
    /// Admit or settle scheduling.
    Schedule,
    /// Validate an execution observation.
    Execute,
    /// Update complete rollout accounting.
    Account,
    /// Compute a statistic or report.
    Analyze,
    /// Apply a domain transition.
    ApplyTransition,
    /// Encode or decode canonical protocol data.
    Codec,
    /// Commit durable state.
    Commit,
    /// Replay or recover state.
    Recover,
    /// Publish a report.
    Publish,
}

/// Stable caller recovery guidance.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum EvaluationRecovery {
    /// Correct caller input and retry with a new command identity.
    CorrectInput,
    /// Reduce configured work under the documented limit.
    ReduceScope,
    /// Replay the authoritative aggregate.
    Replay,
    /// Reconcile a durable external owner before continuing.
    Reconcile,
    /// Retry the exact idempotent operation.
    RetryExact,
    /// Quarantine the campaign for operator inspection.
    Quarantine,
    /// No recovery is available for this terminal result.
    Terminal,
}

/// One bounded redaction-safe E3 error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvaluationError {
    kind: EvaluationErrorKind,
    operation: EvaluationOperation,
    recovery: EvaluationRecovery,
    detail: &'static str,
}

impl EvaluationError {
    /// Creates a stable error from static redaction-reviewed detail.
    #[must_use]
    pub const fn new(
        kind: EvaluationErrorKind,
        operation: EvaluationOperation,
        recovery: EvaluationRecovery,
        detail: &'static str,
    ) -> Self {
        Self { kind, operation, recovery, detail }
    }

    /// Returns the failure category.
    #[must_use]
    pub const fn kind(&self) -> EvaluationErrorKind {
        self.kind
    }

    /// Returns the failed operation.
    #[must_use]
    pub const fn operation(&self) -> EvaluationOperation {
        self.operation
    }

    /// Returns recovery guidance.
    #[must_use]
    pub const fn recovery(&self) -> EvaluationRecovery {
        self.recovery
    }

    /// Returns bounded redaction-safe detail.
    #[must_use]
    pub const fn detail(&self) -> &'static str {
        self.detail
    }
}

impl fmt::Display for EvaluationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "evaluation {:?} failure during {:?}: {} (recovery: {:?})",
            self.kind, self.operation, self.detail, self.recovery
        )
    }
}

impl std::error::Error for EvaluationError {}

#[allow(
    clippy::redundant_pub_crate,
    reason = "shared error constructor used across private implementation modules"
)]
pub(crate) const fn invalid(
    kind: EvaluationErrorKind,
    operation: EvaluationOperation,
    detail: &'static str,
) -> EvaluationError {
    EvaluationError::new(kind, operation, EvaluationRecovery::CorrectInput, detail)
}
