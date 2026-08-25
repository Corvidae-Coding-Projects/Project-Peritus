//! Fresh per-candidate D1 and D3 aggregate identity binding.

use peritus_collaboration::CollaborationBinding;
use peritus_collaboration::CollaborationId;
use peritus_gates::GatePlan;
use peritus_review::ReviewBinding;
use peritus_scheduler::{SchedulerBinding, SchedulerId};
use peritus_types::{RevisionTuple, RunId, Sha256Digest};
use sha2::{Digest, Sha256};

use super::{OrchestratorBinding, hash_revision, reject};
use crate::{OrchestratorError, OrchestratorErrorKind};

/// Per-candidate child aggregate and quality-policy freshness binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityCycleBinding {
    revision: RevisionTuple,
    gate_run_id: RunId,
    scheduler_run_id: RunId,
    collaboration_run_id: RunId,
    gate_plan_digest: Sha256Digest,
    review_binding_digest: Sha256Digest,
    scheduler_id: SchedulerId,
    scheduler_binding_digest: Sha256Digest,
    collaboration_id: CollaborationId,
    collaboration_binding_digest: Sha256Digest,
    digest: Sha256Digest,
}

impl QualityCycleBinding {
    /// Derives one exact cycle from checked D1, D2, and D3 child bindings.
    ///
    /// # Errors
    /// Rejects revision, contract, candidate material, provenance, run, or D3 identity drift.
    pub fn from_children(
        candidate: &crate::CandidateBinding,
        gate_plan: &GatePlan,
        review: &ReviewBinding,
        scheduler: &SchedulerBinding,
        collaboration: &CollaborationBinding,
    ) -> Result<Self, OrchestratorError> {
        let revision = candidate.revision();
        let exact = [
            gate_plan.revision() == revision,
            review.revision() == revision,
            scheduler.revision() == revision,
            collaboration.revision() == revision,
            gate_plan.contract_id() == revision.acceptance_spec_id(),
            review.contract_id() == revision.acceptance_spec_id(),
            gate_plan.contract_digest() == review.contract_digest(),
            review.candidate_digest() == candidate.candidate_digest(),
            review.tree_digest() == candidate.tree_digest(),
            review.producer_actors() == candidate.producer_actors(),
            review.producer_ancestries() == candidate.producer_ancestries(),
            collaboration.scheduler_id() == scheduler.scheduler_id(),
        ]
        .into_iter()
        .all(|matches| matches);
        if !exact {
            return Err(reject(
                OrchestratorErrorKind::BindingMismatch,
                "D1, D2, or D3 child binding differs from the candidate cycle",
            ));
        }
        Self::new(
            revision,
            gate_plan.run_id(),
            scheduler.run_id(),
            collaboration.run_id(),
            gate_plan.digest(),
            review.digest(),
            scheduler.scheduler_id(),
            scheduler.digest(),
            collaboration.id(),
            collaboration.digest(),
        )
    }

    /// Creates one exact candidate-cycle binding.
    ///
    /// # Errors
    /// Rejects zero D1, D2, or D3 binding digests.
    #[allow(clippy::too_many_arguments, reason = "fresh child identities remain explicit")]
    pub fn new(
        revision: RevisionTuple,
        gate_run_id: RunId,
        scheduler_run_id: RunId,
        collaboration_run_id: RunId,
        gate_plan_digest: Sha256Digest,
        review_binding_digest: Sha256Digest,
        scheduler_id: SchedulerId,
        scheduler_binding_digest: Sha256Digest,
        collaboration_id: CollaborationId,
        collaboration_binding_digest: Sha256Digest,
    ) -> Result<Self, OrchestratorError> {
        let mut value = Self::from_wire(
            revision,
            gate_run_id,
            scheduler_run_id,
            collaboration_run_id,
            gate_plan_digest,
            review_binding_digest,
            scheduler_id,
            scheduler_binding_digest,
            collaboration_id,
            collaboration_binding_digest,
            Sha256Digest::new([0; 32]),
        );
        value.validate_shape()?;
        value.digest = quality_cycle_digest(&value);
        Ok(value)
    }

    /// Returns the immutable genesis cycle from the top-level binding.
    #[must_use]
    pub fn genesis(binding: &OrchestratorBinding) -> Self {
        let mut value = Self::from_wire(
            binding.initial_revision,
            binding.initial_gate_run_id,
            binding.initial_scheduler_run_id,
            binding.initial_collaboration_run_id,
            binding.gate_plan_digest,
            binding.review_binding_digest,
            binding.scheduler_id,
            binding.scheduler_binding_digest,
            binding.collaboration_id,
            binding.collaboration_binding_digest,
            Sha256Digest::new([0; 32]),
        );
        value.digest = quality_cycle_digest(&value);
        value
    }

    #[allow(clippy::too_many_arguments, reason = "exact cycle wire fields remain explicit")]
    pub(crate) const fn from_wire(
        revision: RevisionTuple,
        gate_run_id: RunId,
        scheduler_run_id: RunId,
        collaboration_run_id: RunId,
        gate_plan_digest: Sha256Digest,
        review_binding_digest: Sha256Digest,
        scheduler_id: SchedulerId,
        scheduler_binding_digest: Sha256Digest,
        collaboration_id: CollaborationId,
        collaboration_binding_digest: Sha256Digest,
        digest: Sha256Digest,
    ) -> Self {
        Self {
            revision,
            gate_run_id,
            scheduler_run_id,
            collaboration_run_id,
            gate_plan_digest,
            review_binding_digest,
            scheduler_id,
            scheduler_binding_digest,
            collaboration_id,
            collaboration_binding_digest,
            digest,
        }
    }

    pub(crate) fn validate(&self) -> Result<(), OrchestratorError> {
        self.validate_shape()?;
        if self.digest == quality_cycle_digest(self) {
            Ok(())
        } else {
            Err(reject(OrchestratorErrorKind::BindingMismatch, "quality cycle digest differs"))
        }
    }

    /// Checks this child-cycle binding against the exact current candidate revision.
    ///
    /// # Errors
    /// Rejects malformed cycle digests or a cycle for another candidate revision.
    pub fn validate_for_candidate(
        &self,
        candidate: &crate::CandidateBinding,
    ) -> Result<(), OrchestratorError> {
        self.validate()?;
        if self.revision == candidate.revision() {
            Ok(())
        } else {
            Err(reject(
                OrchestratorErrorKind::BindingMismatch,
                "quality cycle revision differs from current candidate",
            ))
        }
    }

    fn validate_shape(&self) -> Result<(), OrchestratorError> {
        let nonzero = [
            self.gate_plan_digest,
            self.review_binding_digest,
            self.scheduler_binding_digest,
            self.collaboration_binding_digest,
        ]
        .iter()
        .all(|digest| digest.as_bytes().iter().any(|byte| *byte != 0));
        let distinct_runs = self.gate_run_id != self.scheduler_run_id
            && self.gate_run_id != self.collaboration_run_id
            && self.scheduler_run_id != self.collaboration_run_id;
        if nonzero && distinct_runs {
            Ok(())
        } else {
            Err(reject(
                OrchestratorErrorKind::InvalidInput,
                "quality cycle digest is zero or child run identities are reused",
            ))
        }
    }

    /// Returns the candidate revision governed by this cycle.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    /// Returns the D1 gate aggregate run for this cycle.
    #[must_use]
    pub const fn gate_run_id(&self) -> RunId {
        self.gate_run_id
    }
    /// Returns the D3 scheduler aggregate run for this cycle.
    #[must_use]
    pub const fn scheduler_run_id(&self) -> RunId {
        self.scheduler_run_id
    }
    /// Returns the D3 collaboration aggregate run for this cycle.
    #[must_use]
    pub const fn collaboration_run_id(&self) -> RunId {
        self.collaboration_run_id
    }
    /// Returns the canonical D1 gate plan digest.
    #[must_use]
    pub const fn gate_plan_digest(&self) -> Sha256Digest {
        self.gate_plan_digest
    }
    /// Returns the canonical D2 review binding digest.
    #[must_use]
    pub const fn review_binding_digest(&self) -> Sha256Digest {
        self.review_binding_digest
    }
    /// Returns the D3 scheduler aggregate identity.
    #[must_use]
    pub const fn scheduler_id(&self) -> SchedulerId {
        self.scheduler_id
    }
    /// Returns the canonical D3 scheduler binding digest.
    #[must_use]
    pub const fn scheduler_binding_digest(&self) -> Sha256Digest {
        self.scheduler_binding_digest
    }
    /// Returns the D3 collaboration aggregate identity.
    #[must_use]
    pub const fn collaboration_id(&self) -> CollaborationId {
        self.collaboration_id
    }
    /// Returns the canonical D3 collaboration binding digest.
    #[must_use]
    pub const fn collaboration_binding_digest(&self) -> Sha256Digest {
        self.collaboration_binding_digest
    }
    /// Returns the canonical digest of the complete cycle binding.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

fn quality_cycle_digest(value: &QualityCycleBinding) -> Sha256Digest {
    let mut hasher = Sha256::new();
    hasher.update(b"peritus.orchestrator.quality-cycle.v1\0");
    hash_revision(&mut hasher, value.revision);
    hasher.update(value.gate_run_id.as_bytes());
    hasher.update(value.scheduler_run_id.as_bytes());
    hasher.update(value.collaboration_run_id.as_bytes());
    hasher.update(value.gate_plan_digest.as_bytes());
    hasher.update(value.review_binding_digest.as_bytes());
    hasher.update(value.scheduler_id.as_bytes());
    hasher.update(value.scheduler_binding_digest.as_bytes());
    hasher.update(value.collaboration_id.as_bytes());
    hasher.update(value.collaboration_binding_digest.as_bytes());
    Sha256Digest::new(hasher.finalize().into())
}
