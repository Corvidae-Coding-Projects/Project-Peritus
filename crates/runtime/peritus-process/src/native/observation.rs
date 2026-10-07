//! Validation of ordered, exact native lifecycle observations.

mod page;

pub(crate) use page::{MAX_PAGE_BYTES, StoredObservationPage};

use peritus_sandbox::{
    CheckedSandboxPlan, EnforcementObservation, ObservationDisposition, ObservationKind,
};
use peritus_types::Sha256Digest;
use sha2::{Digest as _, Sha256};

use super::{NativeObservationTransport, NativeSandboxSession, native_mismatch};
use crate::{
    ErrorCode, ExecutionPlan, ProcessError, ProcessOperation, ProcessStore, RecoveryClass,
    supervisor::SupervisorPlan,
};

pub(crate) fn capture_prepared_session(
    store: &ProcessStore,
    session: &mut dyn NativeSandboxSession,
    execution: &ExecutionPlan,
    sandbox: &CheckedSandboxPlan,
) -> Result<(), ProcessError> {
    let launch = session.launch_description();
    if launch.preparation_digest() != execution.backend().preparation_digest() {
        return Err(native_mismatch("native launch preparation digest differs from admission"));
    }
    capture_and_validate(
        store,
        session,
        execution.identity().process_id(),
        sandbox.digest(),
        execution.backend().descriptor_digest(),
        ObservationStage::Prepared,
    )
}

pub(crate) fn capture_activated_session(
    store: &ProcessStore,
    session: &mut dyn NativeSandboxSession,
    execution: &SupervisorPlan,
    sandbox_digest: Sha256Digest,
) -> Result<(), ProcessError> {
    capture_and_validate(
        store,
        session,
        execution.process_id(),
        sandbox_digest,
        execution.backend_descriptor_digest(),
        ObservationStage::Activated,
    )
}

pub(crate) fn capture_released_pre_start_session(
    store: &ProcessStore,
    session: &mut dyn NativeSandboxSession,
    execution: &ExecutionPlan,
    sandbox_digest: Sha256Digest,
) -> Result<(), ProcessError> {
    capture_and_validate(
        store,
        session,
        execution.identity().process_id(),
        sandbox_digest,
        execution.backend().descriptor_digest(),
        ObservationStage::Released,
    )
}

pub(crate) fn capture_terminated_session(
    store: &ProcessStore,
    session: &mut dyn NativeSandboxSession,
    execution: &SupervisorPlan,
    sandbox_digest: Sha256Digest,
) -> Result<(), ProcessError> {
    capture_and_validate(
        store,
        session,
        execution.process_id(),
        sandbox_digest,
        execution.backend_descriptor_digest(),
        ObservationStage::Terminated,
    )
}

pub(crate) fn capture_released_session(
    store: &ProcessStore,
    session: &mut dyn NativeSandboxSession,
    execution: &SupervisorPlan,
    sandbox_digest: Sha256Digest,
) -> Result<(), ProcessError> {
    capture_and_validate(
        store,
        session,
        execution.process_id(),
        sandbox_digest,
        execution.backend_descriptor_digest(),
        ObservationStage::Released,
    )
}

fn capture_and_validate(
    store: &ProcessStore,
    session: &mut dyn NativeSandboxSession,
    process_id: peritus_types::ProcessId,
    sandbox_digest: Sha256Digest,
    backend_digest: Sha256Digest,
    stage: ObservationStage,
) -> Result<(), ProcessError> {
    let state = store.capture_native_observations(process_id, session)?;
    match (session.observation_transport(), state) {
        (NativeObservationTransport::LegacySnapshot, NativeObservationState::LegacyUnrecorded) => {
            validate_observations(
                session.observations(),
                sandbox_digest,
                backend_digest,
                stage,
            )
        }
        (NativeObservationTransport::DurableDelta, NativeObservationState::Recorded(frontier)) => {
            if frontier.plan_digest != sandbox_digest
                || frontier.backend_digest != backend_digest
                || !stage.accepts(frontier.phase)
            {
                return Err(native_mismatch(
                    "durable native observation frontier is incomplete or incorrectly bound",
                ));
            }
            Ok(())
        }
        _ => Err(native_mismatch(
            "native observation transport differs from its durable manifest state",
        )),
    }
}

#[derive(Clone, Copy)]
enum ObservationStage {
    Prepared,
    Activated,
    Terminated,
    Released,
}

impl ObservationStage {
    fn accepts(self, phase: NativeObservationPhase) -> bool {
        match self {
            Self::Prepared => phase == NativeObservationPhase::Prepared,
            Self::Activated => matches!(
                phase,
                NativeObservationPhase::Activated | NativeObservationPhase::Cancelling
            ),
            Self::Terminated => phase == NativeObservationPhase::Terminated,
            Self::Released => matches!(
                phase,
                NativeObservationPhase::ReleasedFromPrepared
                    | NativeObservationPhase::ReleasedFromTerminated
            ),
        }
    }
}

fn validate_observations(
    observations: &[EnforcementObservation],
    plan_digest: Sha256Digest,
    backend_digest: Sha256Digest,
    stage: ObservationStage,
) -> Result<(), ProcessError> {
    if observations.is_empty() {
        return Err(native_failure("native observation stream is empty"));
    }
    let mut expected_sequence = 1_u64;
    let mut cursor = NativeObservationPhase::AwaitingPrepared;
    for observation in observations.iter().copied() {
        if observation.sequence() != expected_sequence
            || observation.plan_digest() != plan_digest
            || observation.backend_digest() != backend_digest
            || !observation_shape_valid(observation)
        {
            return Err(native_mismatch("native observation binding or sequence is invalid"));
        }
        cursor = cursor.advance(observation.kind())?;
        expected_sequence = expected_sequence
            .checked_add(1)
            .ok_or_else(|| native_mismatch("native observation sequence overflowed"))?;
    }
    if !stage.accepts(cursor) {
        return Err(native_failure("native observation lifecycle is incomplete or out of order"));
    }
    Ok(())
}

pub(super) fn observation_shape_valid(observation: EnforcementObservation) -> bool {
    match observation.kind() {
        ObservationKind::Prepared
        | ObservationKind::Activated
        | ObservationKind::Terminated
        | ObservationKind::Released => {
            observation.disposition() == ObservationDisposition::Completed
        }
        ObservationKind::Cancellation => matches!(
            observation.disposition(),
            ObservationDisposition::Accepted
                | ObservationDisposition::AlreadyAccepted
                | ObservationDisposition::Completed
        ),
        ObservationKind::CapabilityEvaluated
        | ObservationKind::ResourceCharged
        | ObservationKind::FaultInjected => true,
    }
}

/// Exact validated lifecycle frontier retained in the process manifest.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NativeObservationPhase {
    AwaitingPrepared,
    Prepared,
    Activated,
    Cancelling,
    Terminated,
    ReleasedFromPrepared,
    ReleasedFromTerminated,
}

impl NativeObservationPhase {
    pub(crate) const fn advance(self, kind: ObservationKind) -> Result<Self, ProcessError> {
        match (self, kind) {
            (Self::AwaitingPrepared, ObservationKind::Prepared) => Ok(Self::Prepared),
            (Self::Prepared, ObservationKind::Activated) => Ok(Self::Activated),
            (Self::Prepared, ObservationKind::Released) => Ok(Self::ReleasedFromPrepared),
            (Self::Terminated, ObservationKind::Released) => Ok(Self::ReleasedFromTerminated),
            (Self::Prepared | Self::Activated | Self::Cancelling, ObservationKind::Cancellation) => {
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

/// Manifest representation of native observation durability.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NativeObservationState {
    /// No native session has been attached to this execution.
    Untracked,
    /// A pre-V3 manifest or compatibility producer has no exact durable receipt.
    LegacyUnrecorded,
    /// Exact immutable pages exist through this checked frontier.
    Recorded(NativeObservationFrontier),
}

/// Checked cursor for an unbounded immutable native observation log.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct NativeObservationFrontier {
    pub(crate) plan_digest: Sha256Digest,
    pub(crate) backend_digest: Sha256Digest,
    pub(crate) producer_binding_digest: Sha256Digest,
    pub(crate) producer_prefix_digest: Sha256Digest,
    pub(crate) page_count: u64,
    pub(crate) total_count: u64,
    pub(crate) next_sequence: u64,
    pub(crate) last_page_digest: Sha256Digest,
    pub(crate) phase: NativeObservationPhase,
}

/// Binds acknowledgement state to one exact prepared native producer.
#[must_use]
pub fn native_observation_producer_binding(
    manifest_digest: Sha256Digest,
    preparation_digest: Sha256Digest,
) -> Sha256Digest {
    let mut hash = Sha256::new();
    hash.update(b"peritus.native-observation-producer.v1\0");
    hash.update(manifest_digest.as_bytes());
    hash.update(preparation_digest.as_bytes());
    Sha256Digest::new(hash.finalize().into())
}

/// Computes the exact chained digest of one producer's next pending prefix.
#[must_use]
pub fn native_observation_prefix_digest(
    producer_binding: Sha256Digest,
    previous_prefix: Option<Sha256Digest>,
    observations: &[EnforcementObservation],
) -> Sha256Digest {
    let mut hash = Sha256::new();
    hash.update(b"peritus.native-observation-prefix.v1\0");
    hash.update(producer_binding.as_bytes());
    match previous_prefix {
        Some(previous) => {
            hash.update([1]);
            hash.update(previous.as_bytes());
        }
        None => hash.update([0]),
    }
    hash.update(u64::try_from(observations.len()).unwrap_or(u64::MAX).to_be_bytes());
    for observation in observations {
        hash.update(observation.sequence().to_be_bytes());
        hash.update(observation.plan_digest().as_bytes());
        hash.update(observation.backend_digest().as_bytes());
        hash.update([observation_kind_tag(observation.kind())]);
        hash.update([observation_domain_tag(observation.domain())]);
        hash.update([observation_disposition_tag(observation.disposition())]);
    }
    Sha256Digest::new(hash.finalize().into())
}

const fn observation_kind_tag(kind: ObservationKind) -> u8 {
    match kind {
        ObservationKind::Prepared => 1,
        ObservationKind::Activated => 2,
        ObservationKind::CapabilityEvaluated => 3,
        ObservationKind::ResourceCharged => 4,
        ObservationKind::Cancellation => 5,
        ObservationKind::Terminated => 6,
        ObservationKind::Released => 7,
        ObservationKind::FaultInjected => 8,
    }
}

const fn observation_domain_tag(domain: Option<peritus_sandbox::CapabilityDomain>) -> u8 {
    match domain {
        None => 0,
        Some(peritus_sandbox::CapabilityDomain::Filesystem) => 1,
        Some(peritus_sandbox::CapabilityDomain::Process) => 2,
        Some(peritus_sandbox::CapabilityDomain::Environment) => 3,
        Some(peritus_sandbox::CapabilityDomain::Network) => 4,
        Some(peritus_sandbox::CapabilityDomain::Secret) => 5,
        Some(peritus_sandbox::CapabilityDomain::Resource) => 6,
        Some(peritus_sandbox::CapabilityDomain::Terminal) => 7,
    }
}

const fn observation_disposition_tag(disposition: ObservationDisposition) -> u8 {
    match disposition {
        ObservationDisposition::Allowed => 1,
        ObservationDisposition::Denied => 2,
        ObservationDisposition::Completed => 3,
        ObservationDisposition::Accepted => 4,
        ObservationDisposition::AlreadyAccepted => 5,
        ObservationDisposition::Failed => 6,
    }
}

impl NativeObservationFrontier {
    pub(crate) const fn through_sequence(self) -> u64 {
        self.next_sequence - 1
    }
}

pub(crate) const fn native_failure(detail: &'static str) -> ProcessError {
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
