//! One authority for evidence freshness and the qualification it currently supports.

use crate::{
    CandidateCheckpoint, CandidateIdentity, CandidateStage, EvidenceStatus, QualificationEvidence,
    SettlementError,
};
use vstd::prelude::*;

verus! {

impl CandidateCheckpoint {
    /// Reconciles supplied observations and bounds the requested stage by current evidence.
    ///
    /// The caller supplies observed work progress, not a qualification decision. Retained
    /// records keep their original provenance. A fresh negative result may revoke an earlier
    /// qualification even when the workspace snapshot has not changed.
    ///
    /// # Errors
    /// Rejects evidence from a future checkpoint bound to this exact candidate.
    pub fn observe(
        identity: CandidateIdentity,
        requested_stage: CandidateStage,
        gates: EvidenceStatus<QualificationEvidence>,
        obligations: EvidenceStatus<QualificationEvidence>,
        review: EvidenceStatus<QualificationEvidence>,
    ) -> (result: Result<Self, SettlementError>)
        ensures match result {
            Ok(value) => value.spec_identity() == identity
                && value.spec_stage().spec_rank() <= requested_stage.spec_rank()
                && Self::spec_input_error(&value.spec_identity(), value.spec_stage(),
                    &value.spec_gates(), &value.spec_obligations(), &value.spec_review()).is_none(),
            Err(_) => true,
        },
    {
        let gates = gates.reconcile_for(&identity)?;
        let obligations = obligations.reconcile_for(&identity)?;
        let review = review.reconcile_for(&identity)?;
        let stage = supported_stage(
            requested_stage,
            gates.is_current_and_satisfied(&identity),
            obligations.is_current_and_satisfied(&identity),
            review.is_current_and_satisfied(&identity),
        );
        Self::new(identity, stage, gates, obligations, review)
    }

    /// Observes a new binding through the same rules used during active execution.
    ///
    /// # Errors
    /// Rejects foreign lineage, non-advancing sequences, or invalid future evidence.
    pub fn reobserve(&self, identity: CandidateIdentity) -> (result: Result<Self, SettlementError>)
        ensures match result {
            Ok(value) => value.spec_identity() == identity
                && value.spec_successor_error(self).is_none(),
            Err(_) => true,
        },
    {
        let stage = if self.identity().same_candidate(&identity) {
            self.stage()
        } else {
            CandidateStage::Changed
        };
        let current = Self::observe(identity, stage, *self.gates(), *self.obligations(), *self.review())?;
        current.validate_successor(self)?;
        Ok(current)
    }
}

const fn supported_stage(
    requested: CandidateStage,
    gates: bool,
    obligations: bool,
    review: bool,
) -> (stage: CandidateStage)
    ensures
        stage == CandidateStage::Qualified ==> gates && obligations && review,
        stage == CandidateStage::GatesPassed ==> gates,
        stage == CandidateStage::ReviewPending ==> gates && !review,
        stage.spec_rank() <= requested.spec_rank(),
{
    if requested.rank() <= CandidateStage::SelfChecked.rank() {
        requested
    } else if matches!(requested, CandidateStage::Qualified) && gates && obligations && review {
        CandidateStage::Qualified
    } else if !gates {
        CandidateStage::SelfChecked
    } else if matches!(requested, CandidateStage::ReviewPending) && !review {
        CandidateStage::ReviewPending
    } else {
        CandidateStage::GatesPassed
    }
}

} // verus!
