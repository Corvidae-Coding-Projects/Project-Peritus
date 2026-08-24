//! Validation of ordered, exact native lifecycle observations.

use peritus_sandbox::{
    CheckedSandboxPlan, EnforcementObservation, ObservationKind, TeardownCompleteness,
    teardown_completeness,
};
use peritus_types::Sha256Digest;

use super::{NativeSandboxSession, native_mismatch};
use crate::{ErrorCode, ExecutionPlan, ProcessError, ProcessOperation, RecoveryClass};

pub(crate) fn validate_prepared_session(
    session: &dyn NativeSandboxSession,
    execution: &ExecutionPlan,
    sandbox: &CheckedSandboxPlan,
) -> Result<(), ProcessError> {
    let launch = session.launch_description();
    if launch.preparation_digest() != execution.backend().preparation_digest() {
        return Err(native_mismatch("native launch preparation digest differs from admission"));
    }
    validate_observations(
        session.observations(),
        sandbox.digest(),
        execution.backend().descriptor_digest(),
        ObservationStage::Prepared,
    )
}

pub(crate) fn validate_activated_session(
    session: &dyn NativeSandboxSession,
    execution: &ExecutionPlan,
    sandbox_digest: Sha256Digest,
) -> Result<(), ProcessError> {
    validate_observations(
        session.observations(),
        sandbox_digest,
        execution.backend().descriptor_digest(),
        ObservationStage::Activated,
    )
}

pub(crate) fn validate_terminated_session(
    session: &dyn NativeSandboxSession,
    execution: &ExecutionPlan,
    sandbox_digest: Sha256Digest,
) -> Result<(), ProcessError> {
    validate_observations(
        session.observations(),
        sandbox_digest,
        execution.backend().descriptor_digest(),
        ObservationStage::Terminated,
    )
}

pub(crate) fn validate_released_session(
    session: &dyn NativeSandboxSession,
    execution: &ExecutionPlan,
    sandbox_digest: Sha256Digest,
) -> Result<(), ProcessError> {
    validate_observations(
        session.observations(),
        sandbox_digest,
        execution.backend().descriptor_digest(),
        ObservationStage::Released,
    )?;
    let observations = session.observations();
    let prepared_abort_complete = observations.len() >= 2
        && observations.last().map(|value| value.kind()) == Some(ObservationKind::Released)
        && !observations.iter().any(|value| {
            matches!(value.kind(), ObservationKind::Activated | ObservationKind::Terminated)
        });
    if teardown_completeness(observations) != TeardownCompleteness::Complete
        && !prepared_abort_complete
    {
        return Err(native_failure("native session did not prove complete teardown"));
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum ObservationStage {
    Prepared,
    Activated,
    Terminated,
    Released,
}

fn validate_observations(
    observations: &[EnforcementObservation],
    plan_digest: Sha256Digest,
    backend_digest: Sha256Digest,
    stage: ObservationStage,
) -> Result<(), ProcessError> {
    if observations.is_empty() || observations.len() > 4_096 {
        return Err(native_failure("native observation stream is empty or exceeds its bound"));
    }
    for (index, observation) in observations.iter().copied().enumerate() {
        let expected = u64::try_from(index).unwrap_or(u64::MAX).saturating_add(1);
        if observation.sequence() != expected
            || observation.plan_digest() != plan_digest
            || observation.backend_digest() != backend_digest
        {
            return Err(native_mismatch("native observation binding or sequence is invalid"));
        }
    }
    if observations.first().map(|value| value.kind()) != Some(ObservationKind::Prepared) {
        return Err(native_mismatch("native observation stream does not begin with preparation"));
    }
    let mut cursor = LifecycleCursor::Prepared;
    for observation in &observations[1..] {
        cursor = cursor.advance(observation.kind())?;
    }
    let valid = match stage {
        ObservationStage::Prepared => cursor == LifecycleCursor::Prepared,
        ObservationStage::Activated => {
            matches!(cursor, LifecycleCursor::Activated | LifecycleCursor::Cancelling)
        }
        ObservationStage::Terminated => cursor == LifecycleCursor::Terminated,
        ObservationStage::Released => cursor == LifecycleCursor::Released,
    };
    if !valid {
        return Err(native_failure("native observation lifecycle is incomplete or out of order"));
    }
    Ok(())
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum LifecycleCursor {
    Prepared,
    Activated,
    Cancelling,
    Terminated,
    Released,
}

impl LifecycleCursor {
    const fn advance(self, kind: ObservationKind) -> Result<Self, ProcessError> {
        match (self, kind) {
            (Self::Prepared, ObservationKind::Activated) => Ok(Self::Activated),
            (Self::Prepared | Self::Terminated, ObservationKind::Released) => Ok(Self::Released),
            (Self::Activated | Self::Cancelling, ObservationKind::Cancellation) => {
                Ok(Self::Cancelling)
            }
            (Self::Activated | Self::Cancelling, ObservationKind::Terminated) => {
                Ok(Self::Terminated)
            }
            (
                Self::Prepared,
                ObservationKind::CapabilityEvaluated | ObservationKind::FaultInjected,
            )
            | (
                Self::Activated | Self::Cancelling,
                ObservationKind::CapabilityEvaluated
                | ObservationKind::ResourceCharged
                | ObservationKind::FaultInjected,
            ) => Ok(self),
            _ => Err(native_mismatch("native observation lifecycle is duplicated or out of order")),
        }
    }
}

const fn native_failure(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::Supervisor,
        ProcessOperation::Wait,
        RecoveryClass::CancelAndReap,
        detail,
    )
}

#[cfg(test)]
mod tests {
    use peritus_sandbox::{EnforcementObservation, ObservationDisposition, ObservationKind};
    use peritus_types::Sha256Digest;

    use super::{ObservationStage, validate_observations};

    #[test]
    fn native_lifecycle_validation_rejects_duplicate_and_out_of_order_phases() {
        let plan = Sha256Digest::new([1; 32]);
        let backend = Sha256Digest::new([2; 32]);
        let observation = |sequence, kind| {
            EnforcementObservation::new(
                sequence,
                plan,
                backend,
                kind,
                None,
                ObservationDisposition::Completed,
            )
        };
        let duplicate = [
            observation(1, ObservationKind::Prepared),
            observation(2, ObservationKind::Activated),
            observation(3, ObservationKind::Activated),
        ];
        assert!(
            validate_observations(&duplicate, plan, backend, ObservationStage::Activated).is_err()
        );
        let out_of_order = [
            observation(1, ObservationKind::Prepared),
            observation(2, ObservationKind::Terminated),
            observation(3, ObservationKind::Activated),
        ];
        assert!(
            validate_observations(&out_of_order, plan, backend, ObservationStage::Terminated)
                .is_err()
        );
    }
}
