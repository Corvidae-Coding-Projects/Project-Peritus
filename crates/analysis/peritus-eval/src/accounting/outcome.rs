//! Closed truthful rollout outcomes and retained attempts.

use peritus_types::Sha256Digest;

/// Evaluator-confirmed task failure class.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TaskFailureClass {
    /// Candidate output failed a correctness verifier.
    Incorrect,
    /// Candidate output violated a frozen safety verifier.
    Safety,
    /// Candidate output was validly evaluated but incomplete.
    Incomplete,
}

/// Layer responsible for a non-task infrastructure failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum InfrastructureFailureClass {
    /// Scheduler admission, dispatch, or worker ownership failed.
    Scheduler,
    /// C2/C3 process or sandbox execution failed.
    Execution,
    /// C5 provider transport/protocol execution failed.
    Provider,
    /// Hidden evaluator execution failed before a valid verdict.
    Evaluator,
    /// Artifact, trace, or evidence publication failed.
    Publication,
}

/// One logical rollout terminal; task failure requires a valid evaluator digest.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RolloutOutcome {
    /// A valid evaluator confirmed success.
    TaskPassed {
        /// Exact evaluator result digest.
        evaluator_digest: Sha256Digest,
    },
    /// A valid evaluator confirmed task-level failure.
    TaskFailed {
        /// Closed evaluator verdict.
        class: TaskFailureClass,
        /// Exact evaluator result digest.
        evaluator_digest: Sha256Digest,
    },
    /// The candidate/evaluator could not produce a valid task verdict.
    InfrastructureFailed {
        /// Failing infrastructure layer.
        class: InfrastructureFailureClass,
        /// Exact bounded failure record digest.
        failure_digest: Sha256Digest,
        /// Whether exact retry is safe under the frozen contract.
        retryable: bool,
    },
    /// Cancellation settled without a task verdict.
    Cancelled,
    /// External outcome could not be determined safely.
    Ambiguous {
        /// Exact bounded observation describing the unknown outcome.
        observation_digest: Sha256Digest,
    },
}

impl RolloutOutcome {
    /// Returns whether a valid evaluator produced task success.
    #[must_use]
    pub const fn passed(self) -> bool {
        matches!(self, Self::TaskPassed { .. })
    }
    /// Returns whether a valid evaluator produced any task verdict.
    #[must_use]
    pub const fn evaluated(self) -> bool {
        matches!(self, Self::TaskPassed { .. } | Self::TaskFailed { .. })
    }
}

/// One retained execution attempt before logical settlement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RolloutAttempt {
    number: u16,
    observation_digest: Sha256Digest,
    terminal: RolloutOutcome,
    late_after_cancellation: bool,
}

impl RolloutAttempt {
    /// Creates a nonzero retained attempt.
    ///
    /// # Errors
    /// Rejects attempt zero.
    pub const fn new(
        number: u16,
        observation_digest: Sha256Digest,
        terminal: RolloutOutcome,
        late_after_cancellation: bool,
    ) -> Result<Self, crate::EvaluationError> {
        if number == 0 {
            return Err(crate::invalid(
                crate::EvaluationErrorKind::Execution,
                crate::EvaluationOperation::Account,
                "rollout attempt number is zero",
            ));
        }
        Ok(Self { number, observation_digest, terminal, late_after_cancellation })
    }
    /// One-based attempt number.
    #[must_use]
    pub const fn number(self) -> u16 {
        self.number
    }
    /// Exact bounded attempt observation digest.
    #[must_use]
    pub const fn observation_digest(self) -> Sha256Digest {
        self.observation_digest
    }
    /// Attempt terminal observation.
    #[must_use]
    pub const fn terminal(self) -> RolloutOutcome {
        self.terminal
    }
    /// Whether this observation arrived after durable cancellation.
    #[must_use]
    pub const fn late_after_cancellation(self) -> bool {
        self.late_after_cancellation
    }
}
