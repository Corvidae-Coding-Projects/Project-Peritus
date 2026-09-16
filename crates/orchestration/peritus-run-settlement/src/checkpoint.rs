//! Checked candidate checkpoint with current qualification evidence.

use crate::{
    CandidateIdentity, CandidateStage, EvidenceStatus, QualificationEvidence, SettlementError,
    SettlementErrorKind,
};
use vstd::prelude::*;

verus! {

/// Strongest observed state of one exact candidate at one monotonic sequence.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CandidateCheckpoint {
    identity: CandidateIdentity,
    stage: CandidateStage,
    gates: EvidenceStatus<QualificationEvidence>,
    obligations: EvidenceStatus<QualificationEvidence>,
    review: EvidenceStatus<QualificationEvidence>,
}

impl CandidateCheckpoint {
    /// Logical view of the exact candidate identity.
    pub closed spec fn spec_identity(&self) -> CandidateIdentity { self.identity }
    /// Logical view of the declared qualification stage.
    pub closed spec fn spec_stage(&self) -> CandidateStage { self.stage }
    /// Logical view of the retained gate evidence.
    pub closed spec fn spec_gates(&self) -> EvidenceStatus<QualificationEvidence> { self.gates }
    /// Logical view of the retained obligation evidence.
    pub closed spec fn spec_obligations(&self) -> EvidenceStatus<QualificationEvidence> {
        self.obligations
    }
    /// Logical view of the retained review evidence.
    pub closed spec fn spec_review(&self) -> EvidenceStatus<QualificationEvidence> { self.review }

    /// Current and failed observations must bind this exact candidate and checkpoint sequence.
    pub open spec fn spec_evidence_binding_valid(
        evidence: &EvidenceStatus<QualificationEvidence>,
        candidate: &CandidateIdentity,
    ) -> bool {
        match evidence {
            EvidenceStatus::Current(_) | EvidenceStatus::Failed(_) =>
                evidence.spec_is_current_for(candidate),
            EvidenceStatus::Missing | EvidenceStatus::Stale(_) => true,
        }
    }

    /// Exact supplied-evidence support required by the declared stage.
    pub open spec fn spec_stage_supported(
        identity: &CandidateIdentity,
        stage: CandidateStage,
        gates: &EvidenceStatus<QualificationEvidence>,
        obligations: &EvidenceStatus<QualificationEvidence>,
        review: &EvidenceStatus<QualificationEvidence>,
    ) -> bool {
        match stage {
            CandidateStage::Observed | CandidateStage::Changed | CandidateStage::SelfChecked => true,
            CandidateStage::GatesPassed => gates.spec_is_current_and_satisfied(identity),
            CandidateStage::ReviewPending => gates.spec_is_current_and_satisfied(identity)
                && !review.spec_is_current_and_satisfied(identity),
            CandidateStage::Qualified => gates.spec_is_current_and_satisfied(identity)
                && obligations.spec_is_current_and_satisfied(identity)
                && review.spec_is_current_and_satisfied(identity),
        }
    }

    /// Exact constructor rejection category with current, stale, then stage precedence.
    pub open spec fn spec_input_error(
        identity: &CandidateIdentity,
        stage: CandidateStage,
        gates: &EvidenceStatus<QualificationEvidence>,
        obligations: &EvidenceStatus<QualificationEvidence>,
        review: &EvidenceStatus<QualificationEvidence>,
    ) -> Option<SettlementErrorKind> {
        if !Self::spec_evidence_binding_valid(gates, identity)
            || !Self::spec_evidence_binding_valid(obligations, identity)
            || !Self::spec_evidence_binding_valid(review, identity) {
            Some(SettlementErrorKind::CurrentEvidenceBindingMismatch)
        } else if !gates.spec_is_validly_stale_for(identity)
            || !obligations.spec_is_validly_stale_for(identity)
            || !review.spec_is_validly_stale_for(identity) {
            Some(SettlementErrorKind::StaleEvidenceBindingMismatch)
        } else if !Self::spec_stage_supported(identity, stage, gates, obligations, review) {
            Some(SettlementErrorKind::CandidateStageEvidenceMismatch)
        } else {
            None
        }
    }

    /// Qualification requires the Qualified stage and all three positive current observations.
    pub open spec fn spec_is_qualified(&self) -> bool {
        self.spec_stage() == CandidateStage::Qualified
            && self.spec_gates().spec_is_current_and_satisfied(&self.spec_identity())
            && self.spec_obligations().spec_is_current_and_satisfied(&self.spec_identity())
            && self.spec_review().spec_is_current_and_satisfied(&self.spec_identity())
    }

    /// Exact successor rejection category with lineage, sequence, then stage precedence.
    pub open spec fn spec_successor_error(&self, previous: &Self) -> Option<SettlementErrorKind> {
        if !self.spec_identity().spec_same_lineage(&previous.spec_identity()) {
            Some(SettlementErrorKind::CandidateLineageMismatch)
        } else if self.spec_identity().spec_checkpoint_sequence()
            <= previous.spec_identity().spec_checkpoint_sequence() {
            Some(SettlementErrorKind::CheckpointDidNotAdvance)
        } else if self.spec_identity().spec_same_candidate(&previous.spec_identity())
            && self.spec_stage().spec_rank() < previous.spec_stage().spec_rank() {
            Some(SettlementErrorKind::CandidateStageRegressed)
        } else {
            None
        }
    }

    /// Creates a provenance-checked candidate checkpoint.
    ///
    /// # Errors
    ///
    /// Rejects current/failed evidence bound to another candidate, stale evidence that still binds
    /// this candidate, or a declared stage unsupported by positive current evidence.
    pub fn new(
        identity: CandidateIdentity,
        stage: CandidateStage,
        gates: EvidenceStatus<QualificationEvidence>,
        obligations: EvidenceStatus<QualificationEvidence>,
        review: EvidenceStatus<QualificationEvidence>,
    ) -> (result: Result<Self, SettlementError>)
        ensures
            result.is_ok() == Self::spec_input_error(
                &identity, stage, &gates, &obligations, &review).is_none(),
            match result {
                Ok(value) => value.spec_identity() == identity && value.spec_stage() == stage
                    && value.spec_gates() == gates && value.spec_obligations() == obligations
                    && value.spec_review() == review,
                Err(error) => Some(error.spec_kind()) == Self::spec_input_error(
                    &identity, stage, &gates, &obligations, &review),
            },
    {
        if !Self::evidence_binding_valid(&gates, &identity)
            || !Self::evidence_binding_valid(&obligations, &identity)
            || !Self::evidence_binding_valid(&review, &identity)
        {
            return Err(SettlementError::new(
                SettlementErrorKind::CurrentEvidenceBindingMismatch,
            ));
        }
        if !gates.is_validly_stale_for(&identity)
            || !obligations.is_validly_stale_for(&identity)
            || !review.is_validly_stale_for(&identity)
        {
            return Err(SettlementError::new(
                SettlementErrorKind::StaleEvidenceBindingMismatch,
            ));
        }
        let gates_satisfied = gates.is_current_and_satisfied(&identity);
        let obligations_satisfied = obligations.is_current_and_satisfied(&identity);
        let review_satisfied = review.is_current_and_satisfied(&identity);
        let stage_valid = match stage {
            CandidateStage::Observed
            | CandidateStage::Changed
            | CandidateStage::SelfChecked => true,
            CandidateStage::GatesPassed => gates_satisfied,
            CandidateStage::ReviewPending => gates_satisfied && !review_satisfied,
            CandidateStage::Qualified => {
                gates_satisfied && obligations_satisfied && review_satisfied
            }
        };
        if !stage_valid {
            return Err(SettlementError::new(
                SettlementErrorKind::CandidateStageEvidenceMismatch,
            ));
        }
        Ok(Self { identity, stage, gates, obligations, review })
    }

    fn evidence_binding_valid(
        evidence: &EvidenceStatus<QualificationEvidence>,
        candidate: &CandidateIdentity,
    ) -> (valid: bool)
        ensures valid == Self::spec_evidence_binding_valid(evidence, candidate),
    {
        match evidence {
            EvidenceStatus::Current(_) | EvidenceStatus::Failed(_) => {
                evidence.is_current_for(candidate)
            }
            EvidenceStatus::Missing | EvidenceStatus::Stale(_) => true,
        }
    }

    /// Exact candidate identity.
    #[must_use]
    pub const fn identity(&self) -> (value: &CandidateIdentity)
        ensures *value == self.spec_identity(),
    { &self.identity }

    /// Strongest completed stage.
    #[must_use]
    pub const fn stage(&self) -> (value: CandidateStage)
        ensures value == self.spec_stage(),
    { self.stage }

    /// Deterministic-gate evidence.
    #[must_use]
    pub const fn gates(&self) -> (value: &EvidenceStatus<QualificationEvidence>)
        ensures *value == self.spec_gates(),
    { &self.gates }

    /// Public-obligation evidence.
    #[must_use]
    pub const fn obligations(&self) -> (value: &EvidenceStatus<QualificationEvidence>)
        ensures *value == self.spec_obligations(),
    {
        &self.obligations
    }

    /// Independent-review evidence; satisfied means no blocking finding remains.
    #[must_use]
    pub const fn review(&self) -> (value: &EvidenceStatus<QualificationEvidence>)
        ensures *value == self.spec_review(),
    { &self.review }

    /// Whether every acceptance premise is current and satisfied.
    #[must_use]
    pub fn is_qualified(&self) -> (qualified: bool)
        ensures qualified == self.spec_is_qualified(),
    {
        matches!(self.stage, CandidateStage::Qualified)
            && crate::verified::acceptance_allowed(
                true,
                self.gates.is_current_and_satisfied(&self.identity),
                self.obligations.is_current_and_satisfied(&self.identity),
                self.review.is_current_and_satisfied(&self.identity),
            )
    }

    /// Validates that this checkpoint may follow `previous` in one reducer.
    ///
    /// # Errors
    ///
    /// Rejects lineage changes, non-advancing sequences, and stage regression for the same exact
    /// candidate.
    pub fn validate_successor(&self, previous: &Self) -> (result: Result<(), SettlementError>)
        ensures
            result.is_ok() == self.spec_successor_error(previous).is_none(),
            match result {
                Err(error) => Some(error.spec_kind()) == self.spec_successor_error(previous),
                Ok(()) => true,
            },
    {
        if !self.identity.same_lineage(&previous.identity) {
            return Err(SettlementError::new(SettlementErrorKind::CandidateLineageMismatch));
        }
        if self.identity.checkpoint_sequence() <= previous.identity.checkpoint_sequence() {
            return Err(SettlementError::new(SettlementErrorKind::CheckpointDidNotAdvance));
        }
        if self.identity.same_candidate(&previous.identity)
            && !crate::verified::checkpoint_advances(
                previous.identity.checkpoint_sequence(),
                self.identity.checkpoint_sequence(),
                previous.stage.rank(),
                self.stage.rank(),
            )
        {
            return Err(SettlementError::new(SettlementErrorKind::CandidateStageRegressed));
        }
        Ok(())
    }
}

} // verus!
