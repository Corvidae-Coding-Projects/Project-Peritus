//! Interruptible owned execution stages and checked two-stage driver.

use peritus_types::Sha256Digest;

use crate::{
    CandidateExecutionDirective, CandidateObservation, EvaluationError, EvaluationErrorKind,
    EvaluationOperation, EvaluationRecovery, EvaluatorExecutionDirective, EvaluatorObservation,
    ExecutedRollout, ExecutionContinuation, ExecutionFailure, ExecutionStageIdentity,
    FrozenEvaluationProfile, InfrastructureFailureClass, ResourceObservation, RolloutOutcome,
    RolloutSpec, execution::observation::outcome_from_verdict,
};

const CANCELLATION_DOMAIN: &[u8] = b"peritus.evaluation.stage-cancellation.v1\0";
const DRIFT_DOMAIN: &[u8] = b"peritus.evaluation.stage-drift.v1\0";

/// One monotonic observation from the durable cancellation owner.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CancellationObservation {
    revision: u64,
    cancelled: bool,
}

impl CancellationObservation {
    /// Creates an exact cancellation-owner observation.
    #[must_use]
    pub const fn new(revision: u64, cancelled: bool) -> Self {
        Self { revision, cancelled }
    }
    /// Monotonic owner revision.
    #[must_use]
    pub const fn revision(self) -> u64 {
        self.revision
    }
    /// Whether cancellation has durably won.
    #[must_use]
    pub const fn cancelled(self) -> bool {
        self.cancelled
    }
}

/// Read-only monotonic cancellation source owned by runtime composition.
pub trait CancellationProbe {
    /// Returns the current durable cancellation state and its monotonic owner revision.
    fn observe(&self, rollout: crate::RolloutId) -> CancellationObservation;
}

/// Probe used when no cancellation has been requested.
#[derive(Clone, Copy, Debug, Default)]
pub struct NeverCancelled;

impl CancellationProbe for NeverCancelled {
    fn observe(&self, _rollout: crate::RolloutId) -> CancellationObservation {
        CancellationObservation::new(0, false)
    }
}

/// Exact interruption evidence for one owned stage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StageCancellation {
    stage: ExecutionStageIdentity,
    continuation: Option<ExecutionContinuation>,
    observation_digest: Sha256Digest,
    resources: Option<ResourceObservation>,
}

impl StageCancellation {
    /// Interrupted stage identity.
    #[must_use]
    pub const fn stage(self) -> ExecutionStageIdentity {
        self.stage
    }
    /// Same-stage continuation retained by the process owner.
    #[must_use]
    pub const fn continuation(self) -> Option<ExecutionContinuation> {
        self.continuation
    }
    /// Exact cancellation settlement observation.
    #[must_use]
    pub const fn observation_digest(self) -> Sha256Digest {
        self.observation_digest
    }
    /// Resources consumed before interruption, when observed.
    #[must_use]
    pub const fn resources(self) -> Option<ResourceObservation> {
        self.resources
    }
}

/// Result of one interruptible owned-stage call.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StageExecution<T> {
    /// The stage produced a complete observation.
    Completed(T),
    /// The stage failed after retaining any known resources and continuation.
    Failed(ExecutionFailure),
    /// Durable cancellation interrupted this exact stage.
    Cancelled(StageCancellation),
}

impl<T> From<Result<T, ExecutionFailure>> for StageExecution<T> {
    fn from(value: Result<T, ExecutionFailure>) -> Self {
        match value {
            Ok(value) => Self::Completed(value),
            Err(failure) => Self::Failed(failure),
        }
    }
}

/// Cancellation and continuation capability for one exact owned stage.
///
/// Adapters poll [`Self::is_cancelled`] during blocking work and use [`Self::continuation`] when
/// checkpointing. Re-entry for the same effect uses `stage_identity`; it must not create another
/// process/job identity for the same stage.
pub struct ExecutionStageControl<'a> {
    stage: ExecutionStageIdentity,
    cancellation: &'a dyn CancellationProbe,
    observed: CancellationObservation,
    monotonic: bool,
}

impl<'a> ExecutionStageControl<'a> {
    fn new(stage: ExecutionStageIdentity, cancellation: &'a dyn CancellationProbe) -> Self {
        let observed = cancellation.observe(stage.rollout_id());
        Self { stage, cancellation, observed, monotonic: true }
    }
    /// Exact stable stage identity used across process recovery.
    #[must_use]
    pub const fn stage_identity(&self) -> ExecutionStageIdentity {
        self.stage
    }
    /// Derives an exact continuation without changing stage, attempt, job, or effect identity.
    #[must_use]
    pub fn continuation(&self, checkpoint_digest: Sha256Digest) -> ExecutionContinuation {
        ExecutionContinuation::new(self.stage, checkpoint_digest)
    }
    /// Polls the monotonic durable cancellation source.
    ///
    /// A regressing source fails closed as cancelled; the driver rejects it after the stage call.
    pub fn is_cancelled(&mut self) -> bool {
        let next = self.cancellation.observe(self.stage.rollout_id());
        if next.revision() < self.observed.revision()
            || next.revision() == self.observed.revision()
                && next.cancelled() != self.observed.cancelled()
            || self.observed.cancelled() && !next.cancelled()
        {
            self.monotonic = false;
            return true;
        }
        self.observed = next;
        next.cancelled()
    }
    /// Builds explicit interruption evidence if cancellation has won.
    #[must_use]
    pub fn cancellation<T>(
        &mut self,
        checkpoint_digest: Option<Sha256Digest>,
        resources: Option<ResourceObservation>,
    ) -> Option<StageExecution<T>> {
        if !self.is_cancelled() {
            return None;
        }
        let continuation = checkpoint_digest.map(|digest| self.continuation(digest));
        Some(StageExecution::Cancelled(StageCancellation {
            stage: self.stage,
            continuation,
            observation_digest: cancellation_digest(self.stage, self.observed, continuation),
            resources,
        }))
    }
    fn finish(&mut self) -> Result<CancellationObservation, EvaluationError> {
        let _ = self.is_cancelled();
        if self.monotonic {
            Ok(self.observed)
        } else {
            Err(drift("execution cancellation observation regressed"))
        }
    }
}

/// External owner for candidate and separately authorized evaluator execution.
///
/// Both calls receive the stable stage identity, continuation constructor, and live monotonic
/// cancellation source. The adapter owns process interruption, teardown, and same-stage recovery.
/// Each directive also carries the explicit optional deadline; a watchdog never substitutes for
/// cancellation. Failures retain resources already consumed before returning.
pub trait RolloutExecutionPort {
    /// Runs or resumes only the candidate-visible stage.
    fn execute_candidate(
        &mut self,
        directive: &CandidateExecutionDirective,
        control: &mut ExecutionStageControl<'_>,
    ) -> StageExecution<CandidateObservation>;

    /// Runs or resumes the evaluator-only stage after finalized candidate output exists.
    fn execute_evaluator(
        &mut self,
        directive: &EvaluatorExecutionDirective,
        control: &mut ExecutionStageControl<'_>,
    ) -> StageExecution<EvaluatorObservation>;
}

/// Runs candidate then evaluator under the frozen isolation boundary.
///
/// The cancellation source is available inside both blocking effects and is checked again after
/// each effect, including the evaluator. A late ordinary result is retained as partial evidence
/// but produces a cancellation outcome. Invalid effect observations never become task verdicts;
/// they become exact ambiguous observations with their consumed resources retained.
///
/// # Errors
/// Rejects a regressing cancellation source, cross-stage continuation, or invalid failure class.
#[allow(clippy::too_many_lines, reason = "both stage boundaries retain identical explicit ordering")]
pub fn execute_rollout(
    port: &mut impl RolloutExecutionPort,
    cancellation: &impl CancellationProbe,
    spec: &RolloutSpec,
    profile: &FrozenEvaluationProfile,
    attempt: u16,
) -> Result<ExecutedRollout, EvaluationError> {
    let candidate_directive = CandidateExecutionDirective::from_frozen(spec, profile, attempt)?;
    let candidate_stage = candidate_directive.stage_identity();
    let mut candidate_control = ExecutionStageControl::new(candidate_stage, cancellation);
    if candidate_control.is_cancelled() {
        let observed = candidate_control.finish()?;
        return cancelled(
            attempt,
            cancellation_digest(candidate_stage, observed, None),
            None,
            None,
            None,
            None,
            None,
            false,
        );
    }
    let candidate_execution =
        port.execute_candidate(&candidate_directive, &mut candidate_control);
    let candidate_cancellation = candidate_control.finish()?;
    let candidate = match candidate_execution {
        StageExecution::Completed(observation) => {
            let valid = validate_candidate(&candidate_directive, profile, &observation).is_ok();
            if candidate_cancellation.cancelled() {
                return cancelled(
                    attempt,
                    cancellation_digest(candidate_stage, candidate_cancellation, None),
                    valid.then_some(observation),
                    None,
                    Some(observation.resources()),
                    None,
                    None,
                    true,
                );
            }
            if !valid {
                return ambiguous(
                    attempt,
                    candidate_drift_digest(candidate_stage, observation),
                    None,
                    None,
                    Some(observation.resources()),
                    None,
                );
            }
            observation
        }
        StageExecution::Failed(failure) => {
            validate_failure(failure, false, candidate_stage)?;
            if candidate_cancellation.cancelled() {
                return cancelled(
                    attempt,
                    cancellation_digest(
                        candidate_stage,
                        candidate_cancellation,
                        failure.continuation(),
                    ),
                    None,
                    None,
                    failure.resources(),
                    None,
                    failure.continuation(),
                    true,
                );
            }
            return ExecutedRollout::terminal(
                attempt,
                failure.digest(),
                failure.outcome(),
                None,
                None,
                failure.resources(),
                None,
                failure.continuation(),
                false,
            );
        }
        StageExecution::Cancelled(value) => {
            validate_cancellation(value, candidate_stage)?;
            if !candidate_cancellation.cancelled() {
                return Err(drift("candidate stage reported cancellation before its owner"));
            }
            return cancelled(
                attempt,
                value.observation_digest(),
                None,
                None,
                value.resources(),
                None,
                value.continuation(),
                false,
            );
        }
    };

    let evaluator_directive = EvaluatorExecutionDirective::from_candidate(
        &candidate_directive,
        profile,
        candidate.output(),
        candidate.output_bytes(),
    )?;
    let evaluator_stage = evaluator_directive.stage_identity();
    let mut evaluator_control = ExecutionStageControl::new(evaluator_stage, cancellation);
    if evaluator_control.is_cancelled() {
        let observed = evaluator_control.finish()?;
        return cancelled(
            attempt,
            cancellation_digest(evaluator_stage, observed, None),
            Some(candidate),
            None,
            Some(candidate.resources()),
            None,
            None,
            false,
        );
    }
    let evaluator_execution =
        port.execute_evaluator(&evaluator_directive, &mut evaluator_control);
    let evaluator_cancellation = evaluator_control.finish()?;
    match evaluator_execution {
        StageExecution::Completed(evaluator) => {
            let valid = validate_evaluator(&evaluator_directive, evaluator).is_ok();
            if evaluator_cancellation.cancelled() {
                return cancelled(
                    attempt,
                    cancellation_digest(evaluator_stage, evaluator_cancellation, None),
                    Some(candidate),
                    valid.then_some(evaluator),
                    Some(candidate.resources()),
                    Some(evaluator.resources()),
                    None,
                    true,
                );
            }
            if !valid {
                return ambiguous(
                    attempt,
                    evaluator_drift_digest(evaluator_stage, evaluator),
                    Some(candidate),
                    None,
                    Some(candidate.resources()),
                    Some(evaluator.resources()),
                );
            }
            let outcome = outcome_from_verdict(evaluator.verdict(), evaluator.result_digest());
            ExecutedRollout::terminal(
                attempt,
                evaluator.result_digest(),
                outcome,
                Some(candidate),
                Some(evaluator),
                Some(candidate.resources()),
                Some(evaluator.resources()),
                None,
                false,
            )
        }
        StageExecution::Failed(failure) => {
            let failure = evaluator_failure(failure);
            validate_failure(failure, true, evaluator_stage)?;
            if evaluator_cancellation.cancelled() {
                return cancelled(
                    attempt,
                    cancellation_digest(
                        evaluator_stage,
                        evaluator_cancellation,
                        failure.continuation(),
                    ),
                    Some(candidate),
                    None,
                    Some(candidate.resources()),
                    failure.resources(),
                    failure.continuation(),
                    true,
                );
            }
            ExecutedRollout::terminal(
                attempt,
                failure.digest(),
                failure.outcome(),
                Some(candidate),
                None,
                Some(candidate.resources()),
                failure.resources(),
                failure.continuation(),
                false,
            )
        }
        StageExecution::Cancelled(value) => {
            validate_cancellation(value, evaluator_stage)?;
            if !evaluator_cancellation.cancelled() {
                return Err(drift("evaluator stage reported cancellation before its owner"));
            }
            cancelled(
                attempt,
                value.observation_digest(),
                Some(candidate),
                None,
                Some(candidate.resources()),
                value.resources(),
                value.continuation(),
                false,
            )
        }
    }
}

fn validate_candidate(
    directive: &CandidateExecutionDirective,
    profile: &FrozenEvaluationProfile,
    observation: &CandidateObservation,
) -> Result<(), EvaluationError> {
    if observation.rollout_id() != directive.rollout_id()
        || observation.attempt() != directive.attempt()
        || observation.request_digest() != directive.request_digest()
        || observation.observed_execution_digest() != profile.execution().digest()
        || observation.observed_provider_digest() != profile.provider().digest()
        || profile.execution().require_complete_teardown()
            && !observation.resources().teardown_complete()
        || !observation.resources().trace_complete()
    {
        return Err(drift("candidate observation differs from frozen execution bindings"));
    }
    Ok(())
}

fn validate_evaluator(
    directive: &EvaluatorExecutionDirective,
    observation: EvaluatorObservation,
) -> Result<(), EvaluationError> {
    if observation.rollout_id() != directive.rollout_id()
        || observation.attempt() != directive.attempt()
        || observation.request_digest() != directive.request_digest()
        || observation.candidate_output() != directive.candidate_output()
        || observation.observed_execution_digest() != directive.execution().digest()
        || directive.execution().require_complete_teardown()
            && !observation.resources().teardown_complete()
        || !observation.resources().trace_complete()
    {
        return Err(drift("evaluator observation differs from frozen execution bindings"));
    }
    Ok(())
}

fn validate_failure(
    failure: ExecutionFailure,
    evaluator: bool,
    stage: ExecutionStageIdentity,
) -> Result<(), EvaluationError> {
    if evaluator != (failure.class() == InfrastructureFailureClass::Evaluator)
        || failure.continuation().is_some_and(|value| !value.belongs_to(stage))
    {
        return Err(drift("execution failure uses the wrong stage or continuation identity"));
    }
    Ok(())
}

fn validate_cancellation(
    value: StageCancellation,
    stage: ExecutionStageIdentity,
) -> Result<(), EvaluationError> {
    if value.stage() != stage
        || value.continuation().is_some_and(|continuation| !continuation.belongs_to(stage))
    {
        Err(drift("stage cancellation uses another stage or continuation identity"))
    } else {
        Ok(())
    }
}

fn evaluator_failure(failure: ExecutionFailure) -> ExecutionFailure {
    if failure.class() == InfrastructureFailureClass::Evaluator {
        return failure;
    }
    let mut value = ExecutionFailure::new(
        InfrastructureFailureClass::Evaluator,
        failure.digest(),
        failure.retryable(),
    );
    if let Some(resources) = failure.resources() {
        value = value.with_resources(resources);
    }
    if let Some(continuation) = failure.continuation() {
        value = value.with_continuation(continuation);
    }
    value
}

#[allow(clippy::too_many_arguments, reason = "partial stage evidence remains explicit")]
fn cancelled(
    attempt: u16,
    digest: Sha256Digest,
    candidate: Option<CandidateObservation>,
    evaluator: Option<EvaluatorObservation>,
    candidate_resources: Option<ResourceObservation>,
    evaluator_resources: Option<ResourceObservation>,
    continuation: Option<ExecutionContinuation>,
    late: bool,
) -> Result<ExecutedRollout, EvaluationError> {
    ExecutedRollout::terminal(
        attempt,
        digest,
        RolloutOutcome::Cancelled,
        candidate,
        evaluator,
        candidate_resources,
        evaluator_resources,
        continuation,
        late,
    )
}

#[allow(clippy::too_many_arguments, reason = "partial stage evidence remains explicit")]
fn ambiguous(
    attempt: u16,
    digest: Sha256Digest,
    candidate: Option<CandidateObservation>,
    evaluator: Option<EvaluatorObservation>,
    candidate_resources: Option<ResourceObservation>,
    evaluator_resources: Option<ResourceObservation>,
) -> Result<ExecutedRollout, EvaluationError> {
    ExecutedRollout::terminal(
        attempt,
        digest,
        RolloutOutcome::Ambiguous { observation_digest: digest },
        candidate,
        evaluator,
        candidate_resources,
        evaluator_resources,
        None,
        false,
    )
}

fn cancellation_digest(
    stage: ExecutionStageIdentity,
    observation: CancellationObservation,
    continuation: Option<ExecutionContinuation>,
) -> Sha256Digest {
    let mut bytes = Vec::with_capacity(128);
    bytes.extend_from_slice(CANCELLATION_DOMAIN);
    bytes.extend_from_slice(stage.digest().as_bytes());
    bytes.extend_from_slice(&observation.revision().to_be_bytes());
    bytes.push(u8::from(observation.cancelled()));
    bytes.push(u8::from(continuation.is_some()));
    if let Some(value) = continuation {
        bytes.extend_from_slice(value.digest().as_bytes());
    }
    peritus_codec::sha256(&bytes)
}

fn candidate_drift_digest(
    stage: ExecutionStageIdentity,
    observation: CandidateObservation,
) -> Sha256Digest {
    let mut bytes = Vec::with_capacity(224);
    bytes.extend_from_slice(DRIFT_DOMAIN);
    bytes.extend_from_slice(stage.digest().as_bytes());
    bytes.extend_from_slice(observation.rollout_id().as_bytes());
    bytes.extend_from_slice(&observation.attempt().to_be_bytes());
    bytes.extend_from_slice(observation.request_digest().as_bytes());
    bytes.extend_from_slice(observation.output().as_bytes());
    bytes.extend_from_slice(&observation.output_bytes().to_be_bytes());
    bytes.extend_from_slice(observation.observed_execution_digest().as_bytes());
    bytes.extend_from_slice(observation.observed_provider_digest().as_bytes());
    peritus_codec::sha256(&bytes)
}

fn evaluator_drift_digest(
    stage: ExecutionStageIdentity,
    observation: EvaluatorObservation,
) -> Sha256Digest {
    let mut bytes = Vec::with_capacity(192);
    bytes.extend_from_slice(DRIFT_DOMAIN);
    bytes.extend_from_slice(stage.digest().as_bytes());
    bytes.extend_from_slice(observation.rollout_id().as_bytes());
    bytes.extend_from_slice(&observation.attempt().to_be_bytes());
    bytes.extend_from_slice(observation.request_digest().as_bytes());
    bytes.extend_from_slice(observation.candidate_output().as_bytes());
    bytes.extend_from_slice(observation.result_digest().as_bytes());
    bytes.extend_from_slice(observation.observed_execution_digest().as_bytes());
    peritus_codec::sha256(&bytes)
}

const fn drift(detail: &'static str) -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::Execution,
        EvaluationOperation::Execute,
        EvaluationRecovery::Quarantine,
        detail,
    )
}
