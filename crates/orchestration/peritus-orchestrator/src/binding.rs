//! Immutable contract, run, child-domain, revision, and limit binding.

use peritus_collaboration::CollaborationId;
use peritus_scheduler::SchedulerId;
use peritus_spec::AcceptanceContract;
use peritus_types::{AcceptanceSpecId, AttemptId, RevisionTuple, RunId, Sha256Digest};
use sha2::{Digest, Sha256};

mod quality_cycle;

pub use quality_cycle::QualityCycleBinding;

use crate::{
    OrchestratorError, OrchestratorErrorKind, OrchestratorId, OrchestratorLimits,
    OrchestratorRecoveryAction,
};

/// Complete immutable genesis identity for one E0 delivery aggregate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrchestratorBinding {
    id: OrchestratorId,
    run_id: RunId,
    attempt_id: AttemptId,
    contract_id: AcceptanceSpecId,
    contract_digest: Sha256Digest,
    initial_revision: RevisionTuple,
    initial_gate_run_id: RunId,
    initial_scheduler_run_id: RunId,
    initial_collaboration_run_id: RunId,
    contract_gate_cycles: u16,
    contract_review_cycles: u16,
    gate_plan_digest: Sha256Digest,
    review_binding_digest: Sha256Digest,
    scheduler_id: SchedulerId,
    scheduler_binding_digest: Sha256Digest,
    collaboration_id: CollaborationId,
    collaboration_binding_digest: Sha256Digest,
    limits: OrchestratorLimits,
    digest: Sha256Digest,
}

impl OrchestratorBinding {
    /// Binds one checked acceptance contract to all E0 child domains and limits.
    ///
    /// # Errors
    /// Rejects contract/revision mismatch or contract completion limits exceeding E0 bounds.
    #[allow(clippy::too_many_arguments, reason = "immutable cross-domain bindings remain explicit")]
    pub fn from_contract(
        contract: &AcceptanceContract,
        id: OrchestratorId,
        run_id: RunId,
        attempt_id: AttemptId,
        initial_revision: RevisionTuple,
        initial_gate_run_id: RunId,
        initial_scheduler_run_id: RunId,
        initial_collaboration_run_id: RunId,
        gate_plan_digest: Sha256Digest,
        review_binding_digest: Sha256Digest,
        scheduler_id: SchedulerId,
        scheduler_binding_digest: Sha256Digest,
        collaboration_id: CollaborationId,
        collaboration_binding_digest: Sha256Digest,
        limits: OrchestratorLimits,
    ) -> Result<Self, OrchestratorError> {
        let contract_binding = contract.bind(initial_revision).map_err(|_| {
            reject(
                OrchestratorErrorKind::BindingMismatch,
                "acceptance contract does not bind the initial revision",
            )
        })?;
        let completion = contract.completion_policy();
        if completion.max_gate_attempts() > OrchestratorLimits::MAX_GATE_CYCLES
            || completion.max_review_cycles() > OrchestratorLimits::MAX_REVIEW_CYCLES
        {
            return Err(reject(
                OrchestratorErrorKind::LimitExceeded,
                "contract completion limits exceed compiled orchestrator ceilings",
            ));
        }
        let mut value = Self::from_wire(
            id,
            run_id,
            attempt_id,
            contract_binding.contract_id(),
            contract_binding.contract_digest(),
            initial_revision,
            initial_gate_run_id,
            initial_scheduler_run_id,
            initial_collaboration_run_id,
            completion.max_gate_attempts(),
            completion.max_review_cycles(),
            gate_plan_digest,
            review_binding_digest,
            scheduler_id,
            scheduler_binding_digest,
            collaboration_id,
            collaboration_binding_digest,
            limits,
            Sha256Digest::new([0; 32]),
        );
        value.digest = binding_digest(&value);
        value.validate()?;
        Ok(value)
    }

    #[allow(clippy::too_many_arguments, reason = "exact closed-wire binding reconstruction")]
    pub(crate) const fn from_wire(
        id: OrchestratorId,
        run_id: RunId,
        attempt_id: AttemptId,
        contract_id: AcceptanceSpecId,
        contract_digest: Sha256Digest,
        initial_revision: RevisionTuple,
        initial_gate_run_id: RunId,
        initial_scheduler_run_id: RunId,
        initial_collaboration_run_id: RunId,
        contract_gate_cycles: u16,
        contract_review_cycles: u16,
        gate_plan_digest: Sha256Digest,
        review_binding_digest: Sha256Digest,
        scheduler_id: SchedulerId,
        scheduler_binding_digest: Sha256Digest,
        collaboration_id: CollaborationId,
        collaboration_binding_digest: Sha256Digest,
        limits: OrchestratorLimits,
        digest: Sha256Digest,
    ) -> Self {
        Self {
            id,
            run_id,
            attempt_id,
            contract_id,
            contract_digest,
            initial_revision,
            initial_gate_run_id,
            initial_scheduler_run_id,
            initial_collaboration_run_id,
            contract_gate_cycles,
            contract_review_cycles,
            gate_plan_digest,
            review_binding_digest,
            scheduler_id,
            scheduler_binding_digest,
            collaboration_id,
            collaboration_binding_digest,
            limits,
            digest,
        }
    }

    pub(crate) fn validate(&self) -> Result<(), OrchestratorError> {
        self.limits.validate()?;
        let distinct_child_runs = self.initial_gate_run_id != self.initial_scheduler_run_id
            && self.initial_gate_run_id != self.initial_collaboration_run_id
            && self.initial_scheduler_run_id != self.initial_collaboration_run_id;
        if !distinct_child_runs
            || self.contract_id != self.initial_revision.acceptance_spec_id()
            || self.contract_gate_cycles == 0
            || self.contract_gate_cycles > OrchestratorLimits::MAX_GATE_CYCLES
            || self.contract_review_cycles == 0
            || self.contract_review_cycles > OrchestratorLimits::MAX_REVIEW_CYCLES
            || self.gate_plan_digest.as_bytes().iter().all(|byte| *byte == 0)
            || self.review_binding_digest.as_bytes().iter().all(|byte| *byte == 0)
            || self.scheduler_binding_digest.as_bytes().iter().all(|byte| *byte == 0)
            || self.collaboration_binding_digest.as_bytes().iter().all(|byte| *byte == 0)
            || self.digest != binding_digest(self)
        {
            return Err(reject(
                OrchestratorErrorKind::BindingMismatch,
                "orchestrator binding contract, revision, or digest differs",
            ));
        }
        Ok(())
    }

    /// Returns aggregate identity.
    #[must_use]
    pub const fn id(&self) -> OrchestratorId {
        self.id
    }
    /// Returns bound run.
    #[must_use]
    pub const fn run_id(&self) -> RunId {
        self.run_id
    }
    /// Returns bound run attempt.
    #[must_use]
    pub const fn attempt_id(&self) -> AttemptId {
        self.attempt_id
    }
    /// Returns acceptance contract identity.
    #[must_use]
    pub const fn contract_id(&self) -> AcceptanceSpecId {
        self.contract_id
    }
    /// Returns immutable contract digest.
    #[must_use]
    pub const fn contract_digest(&self) -> Sha256Digest {
        self.contract_digest
    }
    /// Returns initial exact revision.
    #[must_use]
    pub const fn initial_revision(&self) -> RevisionTuple {
        self.initial_revision
    }
    /// Returns the immutable genesis D1 child run.
    #[must_use]
    pub const fn initial_gate_run_id(&self) -> RunId {
        self.initial_gate_run_id
    }
    /// Returns the immutable genesis D3 scheduler run.
    #[must_use]
    pub const fn initial_scheduler_run_id(&self) -> RunId {
        self.initial_scheduler_run_id
    }
    /// Returns the immutable genesis D3 collaboration run.
    #[must_use]
    pub const fn initial_collaboration_run_id(&self) -> RunId {
        self.initial_collaboration_run_id
    }
    /// Returns the immutable contract gate-cycle ceiling.
    #[must_use]
    pub const fn contract_gate_cycles(&self) -> u16 {
        self.contract_gate_cycles
    }
    /// Returns the immutable contract review-cycle ceiling.
    #[must_use]
    pub const fn contract_review_cycles(&self) -> u16 {
        self.contract_review_cycles
    }
    /// Returns the effective stricter gate-cycle bound.
    #[must_use]
    pub const fn effective_gate_cycles(&self) -> u16 {
        if self.contract_gate_cycles < self.limits.gate_cycles() {
            self.contract_gate_cycles
        } else {
            self.limits.gate_cycles()
        }
    }
    /// Returns the effective stricter review-cycle bound.
    #[must_use]
    pub const fn effective_review_cycles(&self) -> u16 {
        if self.contract_review_cycles < self.limits.review_cycles() {
            self.contract_review_cycles
        } else {
            self.limits.review_cycles()
        }
    }
    /// Returns D1 plan digest.
    #[must_use]
    pub const fn gate_plan_digest(&self) -> Sha256Digest {
        self.gate_plan_digest
    }
    /// Returns D2 binding digest.
    #[must_use]
    pub const fn review_binding_digest(&self) -> Sha256Digest {
        self.review_binding_digest
    }
    /// Returns D3 scheduler identity.
    #[must_use]
    pub const fn scheduler_id(&self) -> SchedulerId {
        self.scheduler_id
    }
    /// Returns genesis D3 scheduler binding digest.
    #[must_use]
    pub const fn scheduler_binding_digest(&self) -> Sha256Digest {
        self.scheduler_binding_digest
    }
    /// Returns D3 collaboration identity.
    #[must_use]
    pub const fn collaboration_id(&self) -> CollaborationId {
        self.collaboration_id
    }
    /// Returns genesis D3 collaboration binding digest.
    #[must_use]
    pub const fn collaboration_binding_digest(&self) -> Sha256Digest {
        self.collaboration_binding_digest
    }
    /// Returns immutable independent limits.
    #[must_use]
    pub const fn limits(&self) -> OrchestratorLimits {
        self.limits
    }
    /// Returns canonical complete binding digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

fn binding_digest(value: &OrchestratorBinding) -> Sha256Digest {
    let mut hasher = Sha256::new();
    hasher.update(b"peritus.orchestrator.binding.v1\0");
    hasher.update(value.id.as_bytes());
    hasher.update(value.run_id.as_bytes());
    hasher.update(value.attempt_id.as_bytes());
    hasher.update(value.contract_id.as_bytes());
    hasher.update(value.contract_digest.as_bytes());
    hash_revision(&mut hasher, value.initial_revision);
    hasher.update(value.initial_gate_run_id.as_bytes());
    hasher.update(value.initial_scheduler_run_id.as_bytes());
    hasher.update(value.initial_collaboration_run_id.as_bytes());
    hasher.update(value.contract_gate_cycles.to_be_bytes());
    hasher.update(value.contract_review_cycles.to_be_bytes());
    hasher.update(value.gate_plan_digest.as_bytes());
    hasher.update(value.review_binding_digest.as_bytes());
    hasher.update(value.scheduler_id.as_bytes());
    hasher.update(value.scheduler_binding_digest.as_bytes());
    hasher.update(value.collaboration_id.as_bytes());
    hasher.update(value.collaboration_binding_digest.as_bytes());
    hash_limits(&mut hasher, value.limits);
    Sha256Digest::new(hasher.finalize().into())
}

fn hash_revision(hasher: &mut Sha256, revision: RevisionTuple) {
    hasher.update(revision.acceptance_spec_id().as_bytes());
    hasher.update(revision.harness_id().as_bytes());
    hasher.update(revision.workspace_id().as_bytes());
    hasher.update(revision.workspace_generation().get().to_be_bytes());
    hasher.update(revision.workspace_revision().get().to_be_bytes());
    hasher.update(revision.policy_id().as_bytes());
    hasher.update(revision.provider_profile_id().as_bytes());
}

fn hash_limits(hasher: &mut Sha256, limits: OrchestratorLimits) {
    for value in [
        limits.revisions(),
        limits.writer_cycles(),
        limits.fixer_cycles(),
        limits.gate_cycles(),
        limits.review_cycles(),
        limits.handoffs(),
        limits.child_directives(),
        limits.retained_observations(),
        limits.artifact_references(),
        limits.cancellation_reconciliations(),
    ] {
        hasher.update(value.to_be_bytes());
    }
    hasher.update(limits.event_bytes().to_be_bytes());
    hasher.update(limits.state_bytes().to_be_bytes());
}

const fn reject(kind: OrchestratorErrorKind, detail: &'static str) -> OrchestratorError {
    OrchestratorError::new(kind, OrchestratorRecoveryAction::CorrectInput, detail)
}
