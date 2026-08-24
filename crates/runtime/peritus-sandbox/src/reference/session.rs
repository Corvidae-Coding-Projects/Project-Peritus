//! Stateful deterministic reference session.

use super::{ProbeDecision, ReferenceFault, ReferenceFaultPlan, ReferenceProbe};
use crate::{
    BackendDescriptor, CancellationAcceptance, CancellationReason, CancellationState,
    CapabilityDomain, CheckedSandboxPlan, EnforcementObservation, ObservationDisposition,
    ObservationKind, RecoveryClass, SandboxError, SandboxErrorKind, SandboxOperation, SandboxPhase,
    SandboxResourceKind, TeardownCompleteness,
};
use peritus_types::ResourceQuantity;

/// Result of charging one resource dimension.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceDecision {
    /// Usage remains within the checked plan limit.
    WithinLimit,
    /// The named resource limit would be exceeded; usage was not changed.
    LimitExceeded(SandboxResourceKind),
}

/// Terminal result recorded for a reference session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminationKind {
    /// Process exited with an integer status.
    Exited(i32),
    /// Session was cancelled for the accepted reason.
    Cancelled(CancellationReason),
    /// Backend failed before an exit result was available.
    BackendFailure,
}

/// Prepared, deterministic sandbox session with bounded observations.
#[derive(Clone, Debug)]
pub struct ReferenceSession {
    plan: CheckedSandboxPlan,
    descriptor: BackendDescriptor,
    phase: SandboxPhase,
    cancellation: CancellationState,
    usage: crate::ResourceUsage,
    termination: Option<TerminationKind>,
    underlying_exit_status: Option<i32>,
    observations: Vec<EnforcementObservation>,
    observation_limit: usize,
    next_observation_sequence: u64,
    dropped_observations: u64,
    faults: ReferenceFaultPlan,
}

impl ReferenceSession {
    pub(super) fn new(
        plan: CheckedSandboxPlan,
        descriptor: BackendDescriptor,
        faults: ReferenceFaultPlan,
        observation_limit: usize,
    ) -> Result<Self, SandboxError> {
        let mut session = Self {
            plan,
            descriptor,
            phase: SandboxPhase::Prepared,
            cancellation: CancellationState::open(),
            usage: crate::ResourceUsage::zero(),
            termination: None,
            underlying_exit_status: None,
            observations: Vec::new(),
            observation_limit,
            next_observation_sequence: 1,
            dropped_observations: 0,
            faults,
        };
        session.observe_critical(
            ObservationKind::Prepared,
            None,
            ObservationDisposition::Completed,
        )?;
        Ok(session)
    }

    /// Returns checked plan identity.
    #[must_use]
    pub const fn plan(&self) -> &CheckedSandboxPlan {
        &self.plan
    }
    /// Returns backend descriptor.
    #[must_use]
    pub const fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }
    /// Returns current phase.
    #[must_use]
    pub const fn phase(&self) -> SandboxPhase {
        self.phase
    }
    /// Returns cancellation state.
    #[must_use]
    pub const fn cancellation(&self) -> CancellationState {
        self.cancellation
    }
    /// Returns current resource usage.
    #[must_use]
    pub const fn usage(&self) -> crate::ResourceUsage {
        self.usage
    }
    /// Returns terminal result, if recorded.
    #[must_use]
    pub const fn termination(&self) -> Option<TerminationKind> {
        self.termination
    }
    /// Returns an observed numeric exit even when cancellation controls terminal classification.
    #[must_use]
    pub const fn underlying_exit_status(&self) -> Option<i32> {
        self.underlying_exit_status
    }
    /// Returns the ordered observation stream.
    #[must_use]
    pub fn observations(&self) -> &[EnforcementObservation] {
        &self.observations
    }
    /// Returns the number of optional observations not retained because capacity was reserved.
    #[must_use]
    pub const fn dropped_observations(&self) -> u64 {
        self.dropped_observations
    }

    /// Activates a prepared session.
    ///
    /// # Errors
    /// Returns a typed fault or lifecycle error.
    pub fn activate(&mut self) -> Result<(), SandboxError> {
        self.check_fault(ReferenceFault::Activate, SandboxOperation::Activate)?;
        let next = self.phase.transition(SandboxPhase::Active)?;
        self.observe_critical(ObservationKind::Activated, None, ObservationDisposition::Completed)?;
        self.phase = next;
        Ok(())
    }

    /// Evaluates a capability probe against the checked contract.
    ///
    /// # Errors
    /// Returns a typed cancellation, lifecycle, or injected-fault error.
    pub fn evaluate(&mut self, probe: &ReferenceProbe) -> Result<ProbeDecision, SandboxError> {
        self.require_active(SandboxOperation::Evaluate)?;
        self.check_fault(ReferenceFault::Evaluate, SandboxOperation::Evaluate)?;
        let decision = super::evaluation::evaluate(self.plan.contract(), probe);
        let disposition = match decision {
            ProbeDecision::Allowed => ObservationDisposition::Allowed,
            ProbeDecision::Denied => ObservationDisposition::Denied,
        };
        self.observe_optional(
            ObservationKind::CapabilityEvaluated,
            Some(super::evaluation::domain(probe)),
            disposition,
        )?;
        Ok(decision)
    }

    /// Charges a resource dimension using exact checked arithmetic.
    ///
    /// # Errors
    /// Returns a typed cancellation, lifecycle, or injected-fault error.
    pub fn charge(
        &mut self,
        kind: SandboxResourceKind,
        quantity: ResourceQuantity,
    ) -> Result<ResourceDecision, SandboxError> {
        self.require_active(SandboxOperation::Account)?;
        self.check_fault(ReferenceFault::Account, SandboxOperation::Account)?;
        let mut next_usage = self.usage;
        let decision = super::accounting::charge(
            &mut next_usage,
            self.plan.contract().resources(),
            kind,
            quantity,
        );
        let disposition = match decision {
            ResourceDecision::WithinLimit => ObservationDisposition::Allowed,
            ResourceDecision::LimitExceeded(_) => ObservationDisposition::Denied,
        };
        self.observe_optional(
            ObservationKind::ResourceCharged,
            Some(CapabilityDomain::Resource),
            disposition,
        )?;
        self.usage = next_usage;
        Ok(decision)
    }

    /// Requests first-reason-wins cancellation.
    ///
    /// # Errors
    /// Returns a typed fault or lifecycle error.
    pub fn cancel(
        &mut self,
        reason: CancellationReason,
    ) -> Result<CancellationAcceptance, SandboxError> {
        self.check_fault(ReferenceFault::Cancel, SandboxOperation::Cancel)?;
        if !matches!(
            self.phase,
            SandboxPhase::Prepared | SandboxPhase::Active | SandboxPhase::Cancelling
        ) {
            return Err(illegal(
                SandboxOperation::Cancel,
                "session cannot be cancelled in this phase",
            ));
        }
        let mut next_cancellation = self.cancellation;
        let acceptance = next_cancellation.request(reason);
        let next = if acceptance == CancellationAcceptance::Accepted {
            Some(self.phase.transition(SandboxPhase::Cancelling)?)
        } else {
            None
        };
        let disposition = match acceptance {
            CancellationAcceptance::Accepted => ObservationDisposition::Accepted,
            CancellationAcceptance::AlreadyAccepted => ObservationDisposition::AlreadyAccepted,
        };
        if acceptance == CancellationAcceptance::Accepted {
            self.observe_critical(ObservationKind::Cancellation, None, disposition)?;
        } else {
            self.observe_optional(ObservationKind::Cancellation, None, disposition)?;
        }
        if let Some(next) = next {
            self.cancellation = next_cancellation;
            self.phase = next;
        }
        Ok(acceptance)
    }

    /// Records a terminal result.
    ///
    /// # Errors
    /// Returns a typed fault, lifecycle, or consistency error.
    pub fn terminate(&mut self, result: TerminationKind) -> Result<(), SandboxError> {
        self.check_fault(ReferenceFault::Terminate, SandboxOperation::Terminate)?;
        let accepted_reason = self.cancellation.reason();
        if matches!(result, TerminationKind::Cancelled(_)) && accepted_reason.is_none() {
            return Err(illegal(
                SandboxOperation::Terminate,
                "cancelled result has no accepted cancellation",
            ));
        }
        let underlying_exit_status = match result {
            TerminationKind::Exited(status) => Some(status),
            _ => None,
        };
        let effective = accepted_reason.map_or(result, TerminationKind::Cancelled);
        let next = self.phase.transition(SandboxPhase::Terminated)?;
        self.observe_critical(
            ObservationKind::Terminated,
            None,
            ObservationDisposition::Completed,
        )?;
        self.phase = next;
        self.underlying_exit_status = underlying_exit_status;
        self.termination = Some(effective);
        Ok(())
    }

    /// Releases a terminated session.
    ///
    /// # Errors
    /// Returns a typed fault or lifecycle error.
    pub fn release(&mut self) -> Result<(), SandboxError> {
        self.check_fault(ReferenceFault::Release, SandboxOperation::Release)?;
        let next = self.phase.transition(SandboxPhase::Released)?;
        self.observe_critical(ObservationKind::Released, None, ObservationDisposition::Completed)?;
        self.phase = next;
        Ok(())
    }

    /// Reports whether termination and release evidence is complete.
    #[must_use]
    pub fn teardown_completeness(&self) -> TeardownCompleteness {
        crate::observation::teardown_completeness(&self.observations)
    }

    fn require_active(&self, operation: SandboxOperation) -> Result<(), SandboxError> {
        if self.cancellation.is_cancelled() {
            return Err(SandboxError::new(
                SandboxErrorKind::Cancelled,
                operation,
                RecoveryClass::CancelAndRelease,
                "sandbox session is cancelled",
            ));
        }
        if self.phase != SandboxPhase::Active {
            return Err(illegal(operation, "sandbox session is not active"));
        }
        Ok(())
    }

    const fn check_fault(
        &self,
        fault: ReferenceFault,
        operation: SandboxOperation,
    ) -> Result<(), SandboxError> {
        if self.faults.contains(fault) { Err(crate::error::injected(operation)) } else { Ok(()) }
    }

    fn observe_optional(
        &mut self,
        kind: ObservationKind,
        domain: Option<CapabilityDomain>,
        disposition: ObservationDisposition,
    ) -> Result<(), SandboxError> {
        self.observe(kind, domain, disposition, false)
    }

    fn observe_critical(
        &mut self,
        kind: ObservationKind,
        domain: Option<CapabilityDomain>,
        disposition: ObservationDisposition,
    ) -> Result<(), SandboxError> {
        self.observe(kind, domain, disposition, true)
    }

    fn observe(
        &mut self,
        kind: ObservationKind,
        domain: Option<CapabilityDomain>,
        disposition: ObservationDisposition,
        critical: bool,
    ) -> Result<(), SandboxError> {
        if self.faults.contains(ReferenceFault::Observation) {
            return Err(crate::error::injected(SandboxOperation::Evaluate));
        }
        let sequence = self.next_observation_sequence;
        self.next_observation_sequence = self.next_observation_sequence.saturating_add(1);
        let retention_ceiling = if critical {
            self.observation_limit
        } else {
            self.observation_limit.saturating_sub(self.reserved_lifecycle_events())
        };
        if self.observations.len() >= retention_ceiling {
            self.dropped_observations = self.dropped_observations.saturating_add(1);
            return Ok(());
        }
        self.observations.push(EnforcementObservation::new(
            sequence,
            self.plan.digest(),
            self.descriptor.digest(),
            kind,
            domain,
            disposition,
        ));
        Ok(())
    }

    const fn reserved_lifecycle_events(&self) -> usize {
        match self.phase {
            SandboxPhase::Planned => 5,
            SandboxPhase::Prepared => 4,
            SandboxPhase::Active => 3,
            SandboxPhase::Cancelling => 2,
            SandboxPhase::Terminated => 1,
            SandboxPhase::Released => 0,
        }
    }
}

const fn illegal(operation: SandboxOperation, detail: &'static str) -> SandboxError {
    SandboxError::new(
        SandboxErrorKind::IllegalTransition,
        operation,
        RecoveryClass::Reconcile,
        detail,
    )
}
