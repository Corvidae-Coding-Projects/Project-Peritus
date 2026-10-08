//! Checked output, verdict, failure, and resource observations.

use peritus_artifact_store::ArtifactDigest;
use peritus_types::Sha256Digest;

use crate::{
    EvaluationError, EvaluationErrorKind, EvaluationOperation, ExecutionContinuation,
    InfrastructureFailureClass, ResourceObservation, RolloutAttempt, RolloutId, RolloutOutcome,
    TaskFailureClass,
};

/// Typed redaction-safe execution failure returned by an external owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExecutionFailure {
    class: InfrastructureFailureClass,
    digest: Sha256Digest,
    retryable: bool,
    resources: Option<ResourceObservation>,
    continuation: Option<ExecutionContinuation>,
}

impl ExecutionFailure {
    /// Creates one exact infrastructure failure observation.
    #[must_use]
    pub const fn new(
        class: InfrastructureFailureClass,
        digest: Sha256Digest,
        retryable: bool,
    ) -> Self {
        Self { class, digest, retryable, resources: None, continuation: None }
    }
    /// Retains resources consumed before the failed stage stopped.
    #[must_use]
    pub const fn with_resources(mut self, resources: ResourceObservation) -> Self {
        self.resources = Some(resources);
        self
    }
    /// Retains the exact owner checkpoint for resuming this same stage and effect.
    #[must_use]
    pub const fn with_continuation(mut self, continuation: ExecutionContinuation) -> Self {
        self.continuation = Some(continuation);
        self
    }
    /// Responsible layer.
    #[must_use]
    pub const fn class(self) -> InfrastructureFailureClass {
        self.class
    }
    /// Bounded failure record digest.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.digest
    }
    /// Whether exact retry is safe.
    #[must_use]
    pub const fn retryable(self) -> bool {
        self.retryable
    }
    /// Resources consumed before failure, when observed.
    #[must_use]
    pub const fn resources(self) -> Option<ResourceObservation> {
        self.resources
    }
    /// Exact same-stage continuation, when the owner produced one.
    #[must_use]
    pub const fn continuation(self) -> Option<ExecutionContinuation> {
        self.continuation
    }

    pub(crate) const fn outcome(self) -> RolloutOutcome {
        RolloutOutcome::InfrastructureFailed {
            class: self.class,
            failure_digest: self.digest,
            retryable: self.retryable,
        }
    }
}

/// Finalized candidate output and exact fidelity observations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CandidateObservation {
    rollout_id: RolloutId,
    attempt: u16,
    request_digest: Sha256Digest,
    output: ArtifactDigest,
    output_bytes: u64,
    observed_execution_digest: Sha256Digest,
    observed_provider_digest: Sha256Digest,
    resources: ResourceObservation,
}

impl CandidateObservation {
    /// Constructs a bounded candidate observation.
    ///
    /// # Errors
    /// Rejects attempt zero or empty output.
    #[allow(clippy::too_many_arguments, reason = "all fidelity observations remain explicit")]
    pub const fn new(
        rollout_id: RolloutId,
        attempt: u16,
        request_digest: Sha256Digest,
        output: ArtifactDigest,
        output_bytes: u64,
        observed_execution_digest: Sha256Digest,
        observed_provider_digest: Sha256Digest,
        resources: ResourceObservation,
    ) -> Result<Self, EvaluationError> {
        if attempt == 0 || output_bytes == 0 {
            return Err(invalid("candidate observation attempt or output size is zero"));
        }
        Ok(Self {
            rollout_id,
            attempt,
            request_digest,
            output,
            output_bytes,
            observed_execution_digest,
            observed_provider_digest,
            resources,
        })
    }
    /// Logical rollout.
    #[must_use]
    pub const fn rollout_id(self) -> RolloutId {
        self.rollout_id
    }
    /// Attempt number.
    #[must_use]
    pub const fn attempt(self) -> u16 {
        self.attempt
    }
    /// Planned request digest observed by the adapter.
    #[must_use]
    pub const fn request_digest(self) -> Sha256Digest {
        self.request_digest
    }
    /// Finalized output artifact.
    #[must_use]
    pub const fn output(self) -> ArtifactDigest {
        self.output
    }
    /// Exact output bytes.
    #[must_use]
    pub const fn output_bytes(self) -> u64 {
        self.output_bytes
    }
    /// Observed C2/C3 binding digest.
    #[must_use]
    pub const fn observed_execution_digest(self) -> Sha256Digest {
        self.observed_execution_digest
    }
    /// Observed C5 profile digest.
    #[must_use]
    pub const fn observed_provider_digest(self) -> Sha256Digest {
        self.observed_provider_digest
    }
    /// Candidate-stage resources.
    #[must_use]
    pub const fn resources(self) -> ResourceObservation {
        self.resources
    }
}

/// Valid evaluator result; infrastructure failures are represented separately.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvaluatorVerdict {
    /// Candidate satisfied the frozen verifier.
    Passed,
    /// Candidate failed a frozen task constraint.
    Failed(TaskFailureClass),
}

/// Evaluator result and exact fidelity observations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvaluatorObservation {
    rollout_id: RolloutId,
    attempt: u16,
    request_digest: Sha256Digest,
    candidate_output: ArtifactDigest,
    verdict: EvaluatorVerdict,
    result_digest: Sha256Digest,
    observed_execution_digest: Sha256Digest,
    resources: ResourceObservation,
}

impl EvaluatorObservation {
    /// Constructs one evaluator verdict bound to its candidate output.
    ///
    /// # Errors
    /// Rejects attempt zero.
    #[allow(
        clippy::too_many_arguments,
        reason = "all evaluator fidelity observations stay explicit"
    )]
    pub const fn new(
        rollout_id: RolloutId,
        attempt: u16,
        request_digest: Sha256Digest,
        candidate_output: ArtifactDigest,
        verdict: EvaluatorVerdict,
        result_digest: Sha256Digest,
        observed_execution_digest: Sha256Digest,
        resources: ResourceObservation,
    ) -> Result<Self, EvaluationError> {
        if attempt == 0 {
            return Err(invalid("evaluator observation attempt is zero"));
        }
        Ok(Self {
            rollout_id,
            attempt,
            request_digest,
            candidate_output,
            verdict,
            result_digest,
            observed_execution_digest,
            resources,
        })
    }
    /// Logical rollout.
    #[must_use]
    pub const fn rollout_id(self) -> RolloutId {
        self.rollout_id
    }
    /// Attempt number.
    #[must_use]
    pub const fn attempt(self) -> u16 {
        self.attempt
    }
    /// Planned request digest.
    #[must_use]
    pub const fn request_digest(self) -> Sha256Digest {
        self.request_digest
    }
    /// Candidate output inspected by the evaluator.
    #[must_use]
    pub const fn candidate_output(self) -> ArtifactDigest {
        self.candidate_output
    }
    /// Valid evaluator verdict.
    #[must_use]
    pub const fn verdict(self) -> EvaluatorVerdict {
        self.verdict
    }
    /// Canonical evaluator-result digest.
    #[must_use]
    pub const fn result_digest(self) -> Sha256Digest {
        self.result_digest
    }
    /// Observed evaluator C2/C3 binding digest.
    #[must_use]
    pub const fn observed_execution_digest(self) -> Sha256Digest {
        self.observed_execution_digest
    }
    /// Evaluator-stage resources.
    #[must_use]
    pub const fn resources(self) -> ResourceObservation {
        self.resources
    }
}

/// Complete checked result ready for durable attempt and terminal settlement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExecutedRollout {
    attempt: RolloutAttempt,
    candidate: Option<CandidateObservation>,
    evaluator: Option<EvaluatorObservation>,
    candidate_resources: Option<ResourceObservation>,
    evaluator_resources: Option<ResourceObservation>,
    continuation: Option<ExecutionContinuation>,
}

impl ExecutedRollout {
    #[allow(
        clippy::large_types_passed_by_value,
        reason = "the fixed-size checked observations are consumed exactly once into the terminal record"
    )]
    pub(crate) fn terminal(
        number: u16,
        observation_digest: Sha256Digest,
        outcome: RolloutOutcome,
        candidate: Option<CandidateObservation>,
        evaluator: Option<EvaluatorObservation>,
        candidate_resources: Option<ResourceObservation>,
        evaluator_resources: Option<ResourceObservation>,
        continuation: Option<ExecutionContinuation>,
        late_after_cancellation: bool,
    ) -> Result<Self, EvaluationError> {
        Ok(Self {
            attempt: RolloutAttempt::new(
                number,
                observation_digest,
                outcome,
                late_after_cancellation,
            )?,
            candidate,
            evaluator,
            candidate_resources: candidate_resources
                .or_else(|| candidate.map(CandidateObservation::resources)),
            evaluator_resources: evaluator_resources
                .or_else(|| evaluator.map(EvaluatorObservation::resources)),
            continuation,
        })
    }
    /// Retained attempt and logical terminal.
    #[must_use]
    pub const fn attempt(self) -> RolloutAttempt {
        self.attempt
    }
    /// Candidate output when the first stage completed.
    #[must_use]
    pub const fn candidate(self) -> Option<CandidateObservation> {
        self.candidate
    }
    /// Valid evaluator observation when the second stage completed.
    #[must_use]
    pub const fn evaluator(self) -> Option<EvaluatorObservation> {
        self.evaluator
    }
    /// Candidate resources, including partial resources from a failed/interrupted stage.
    #[must_use]
    pub const fn candidate_resources(self) -> Option<ResourceObservation> {
        self.candidate_resources
    }
    /// Evaluator resources, including partial resources from a failed/interrupted stage.
    #[must_use]
    pub const fn evaluator_resources(self) -> Option<ResourceObservation> {
        self.evaluator_resources
    }
    /// Exact same-stage continuation retained before cancellation, reconciliation, or retry.
    #[must_use]
    pub const fn continuation(self) -> Option<ExecutionContinuation> {
        self.continuation
    }
}

pub(super) const fn outcome_from_verdict(
    verdict: EvaluatorVerdict,
    digest: Sha256Digest,
) -> RolloutOutcome {
    match verdict {
        EvaluatorVerdict::Passed => RolloutOutcome::TaskPassed { evaluator_digest: digest },
        EvaluatorVerdict::Failed(class) => {
            RolloutOutcome::TaskFailed { class, evaluator_digest: digest }
        }
    }
}

const fn invalid(detail: &'static str) -> EvaluationError {
    crate::invalid(EvaluationErrorKind::Execution, EvaluationOperation::Execute, detail)
}
