//! External execution port and checked two-stage driver.

use peritus_types::Sha256Digest;

use crate::{
    CandidateExecutionDirective, CandidateObservation, EvaluationError, EvaluationErrorKind,
    EvaluationOperation, EvaluationRecovery, EvaluatorExecutionDirective, EvaluatorObservation,
    ExecutedRollout, ExecutionFailure, FrozenEvaluationProfile, InfrastructureFailureClass,
    RolloutOutcome, RolloutSpec, execution::observation::outcome_from_verdict,
};

/// External owner for candidate and separately authorized evaluator execution.
pub trait RolloutExecutionPort {
    /// Runs only the candidate-visible stage.
    ///
    /// # Errors
    /// Returns a typed infrastructure observation when the candidate stage cannot complete.
    fn execute_candidate(
        &mut self,
        directive: &CandidateExecutionDirective,
    ) -> Result<CandidateObservation, ExecutionFailure>;

    /// Runs the evaluator-only stage after finalized candidate output exists.
    ///
    /// # Errors
    /// Returns a typed infrastructure observation when the evaluator stage cannot complete.
    fn execute_evaluator(
        &mut self,
        directive: &EvaluatorExecutionDirective,
    ) -> Result<EvaluatorObservation, ExecutionFailure>;
}

/// Read-only cancellation observation owned by the runtime composition layer.
pub trait CancellationProbe {
    /// Whether durable cancellation currently wins for this rollout.
    fn cancelled(&self, rollout: crate::RolloutId) -> bool;
}

/// Probe used when no cancellation has been requested.
#[derive(Clone, Copy, Debug, Default)]
pub struct NeverCancelled;

impl CancellationProbe for NeverCancelled {
    fn cancelled(&self, _rollout: crate::RolloutId) -> bool {
        false
    }
}

/// Runs candidate then evaluator under the frozen isolation boundary.
///
/// Adapter failures become explicit infrastructure outcomes; only a valid evaluator observation
/// can become a task verdict. Cancellation is checked before each effect.
///
/// # Errors
/// Rejects adapter observations that drift from the exact directive/profile bindings.
pub fn execute_rollout(
    port: &mut impl RolloutExecutionPort,
    cancellation: &impl CancellationProbe,
    spec: &RolloutSpec,
    profile: &FrozenEvaluationProfile,
    attempt: u16,
) -> Result<ExecutedRollout, EvaluationError> {
    if cancellation.cancelled(spec.id()) {
        return ExecutedRollout::terminal(
            attempt,
            digest_cancelled(spec, attempt),
            RolloutOutcome::Cancelled,
            None,
            None,
        );
    }
    let candidate_directive = CandidateExecutionDirective::from_frozen(spec, profile, attempt)?;
    let candidate = match port.execute_candidate(&candidate_directive) {
        Ok(value) => value,
        Err(failure) => {
            validate_failure(failure, false)?;
            return ExecutedRollout::terminal(
                attempt,
                failure.digest(),
                failure.outcome(),
                None,
                None,
            );
        }
    };
    validate_candidate(&candidate_directive, profile, &candidate)?;
    if cancellation.cancelled(spec.id()) {
        return ExecutedRollout::terminal(
            attempt,
            digest_cancelled(spec, attempt),
            RolloutOutcome::Cancelled,
            Some(candidate),
            None,
        );
    }
    let evaluator_directive = EvaluatorExecutionDirective::from_candidate(
        &candidate_directive,
        profile,
        candidate.output(),
        candidate.output_bytes(),
    )?;
    let evaluator = match port.execute_evaluator(&evaluator_directive) {
        Ok(value) => value,
        Err(mut failure) => {
            if failure.class() != InfrastructureFailureClass::Evaluator {
                failure = ExecutionFailure::new(
                    InfrastructureFailureClass::Evaluator,
                    failure.digest(),
                    failure.retryable(),
                );
            }
            return ExecutedRollout::terminal(
                attempt,
                failure.digest(),
                failure.outcome(),
                Some(candidate),
                None,
            );
        }
    };
    validate_evaluator(&evaluator_directive, evaluator)?;
    let outcome = outcome_from_verdict(evaluator.verdict(), evaluator.result_digest());
    ExecutedRollout::terminal(
        attempt,
        evaluator.result_digest(),
        outcome,
        Some(candidate),
        Some(evaluator),
    )
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

fn validate_failure(failure: ExecutionFailure, evaluator: bool) -> Result<(), EvaluationError> {
    if evaluator != (failure.class() == InfrastructureFailureClass::Evaluator) {
        return Err(drift("execution failure uses the wrong stage classification"));
    }
    Ok(())
}

fn digest_cancelled(spec: &RolloutSpec, attempt: u16) -> Sha256Digest {
    let mut bytes = b"peritus.evaluation.cancelled-attempt.v1\0".to_vec();
    bytes.extend_from_slice(spec.id().as_bytes());
    bytes.extend_from_slice(&attempt.to_be_bytes());
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
