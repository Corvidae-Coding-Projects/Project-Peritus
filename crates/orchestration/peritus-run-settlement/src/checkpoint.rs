//! Checked candidate checkpoint with current qualification evidence.

use crate::{
    CandidateIdentity, CandidateStage, EvidenceStatus, QualificationEvidence, SettlementError,
    SettlementErrorKind,
};

#[doc = " Currently supported qualification of one exact candidate at one monotonic sequence."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CandidateCheckpoint {
    identity: CandidateIdentity,
    stage: CandidateStage,
    gates: EvidenceStatus<QualificationEvidence>,
    obligations: EvidenceStatus<QualificationEvidence>,
    review: EvidenceStatus<QualificationEvidence>,
}
impl CandidateCheckpoint {
    #[doc = " Creates a provenance-checked candidate checkpoint."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Rejects current/failed evidence bound to another candidate, stale evidence that still binds"]
    #[doc = " this candidate, or a declared stage unsupported by positive current evidence."]
    pub fn new(
        identity: CandidateIdentity,
        stage: CandidateStage,
        gates: EvidenceStatus<QualificationEvidence>,
        obligations: EvidenceStatus<QualificationEvidence>,
        review: EvidenceStatus<QualificationEvidence>,
    ) -> Result<Self, SettlementError> {
        if !Self::evidence_binding_valid(&gates, &identity)
            || !Self::evidence_binding_valid(&obligations, &identity)
            || !Self::evidence_binding_valid(&review, &identity)
        {
            return Err(SettlementError::new(SettlementErrorKind::CurrentEvidenceBindingMismatch));
        }
        if !gates.is_validly_stale_for(&identity)
            || !obligations.is_validly_stale_for(&identity)
            || !review.is_validly_stale_for(&identity)
        {
            return Err(SettlementError::new(SettlementErrorKind::StaleEvidenceBindingMismatch));
        }
        let gates_satisfied = gates.is_current_and_satisfied(&identity);
        let obligations_satisfied = obligations.is_current_and_satisfied(&identity);
        let review_satisfied = review.is_current_and_satisfied(&identity);
        let stage_valid = match stage {
            CandidateStage::Observed | CandidateStage::Changed | CandidateStage::SelfChecked => {
                true
            }
            CandidateStage::GatesPassed => gates_satisfied,
            CandidateStage::ReviewPending => gates_satisfied && !review_satisfied,
            CandidateStage::Qualified => {
                gates_satisfied && obligations_satisfied && review_satisfied
            }
        };
        if !stage_valid {
            return Err(SettlementError::new(SettlementErrorKind::CandidateStageEvidenceMismatch));
        }
        Ok(Self { identity, stage, gates, obligations, review })
    }
    fn evidence_binding_valid(
        evidence: &EvidenceStatus<QualificationEvidence>,
        candidate: &CandidateIdentity,
    ) -> bool {
        match evidence {
            EvidenceStatus::Current(_) | EvidenceStatus::Failed(_) => {
                evidence.is_current_for(candidate)
            }
            EvidenceStatus::Missing | EvidenceStatus::Stale(_) => true,
        }
    }
    #[doc = " Exact candidate identity."]
    #[must_use]
    pub const fn identity(&self) -> &CandidateIdentity {
        &self.identity
    }
    #[doc = " Currently supported qualification stage."]
    #[must_use]
    pub const fn stage(&self) -> CandidateStage {
        self.stage
    }
    #[doc = " Deterministic-gate evidence."]
    #[must_use]
    pub const fn gates(&self) -> &EvidenceStatus<QualificationEvidence> {
        &self.gates
    }
    #[doc = " Public-obligation evidence."]
    #[must_use]
    pub const fn obligations(&self) -> &EvidenceStatus<QualificationEvidence> {
        &self.obligations
    }
    #[doc = " Independent-review evidence; satisfied means no blocking finding remains."]
    #[must_use]
    pub const fn review(&self) -> &EvidenceStatus<QualificationEvidence> {
        &self.review
    }
    #[doc = " Whether every acceptance premise is current and satisfied."]
    #[must_use]
    pub fn is_qualified(&self) -> bool {
        matches!(self.stage, CandidateStage::Qualified)
            && crate::verified::acceptance_allowed(
                true,
                self.gates.is_current_and_satisfied(&self.identity),
                self.obligations.is_current_and_satisfied(&self.identity),
                self.review.is_current_and_satisfied(&self.identity),
            )
    }
    #[doc = " Validates that this checkpoint may follow `previous` in one reducer."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Rejects lineage changes and non-advancing sequences. New observations may reduce"]
    #[doc = " qualification for unchanged content; current evidence remains checked by construction."]
    pub fn validate_successor(&self, previous: &Self) -> Result<(), SettlementError> {
        if !self.identity.same_lineage(&previous.identity) {
            return Err(SettlementError::new(SettlementErrorKind::CandidateLineageMismatch));
        }
        if self.identity.checkpoint_sequence() <= previous.identity.checkpoint_sequence() {
            return Err(SettlementError::new(SettlementErrorKind::CheckpointDidNotAdvance));
        }
        Ok(())
    }
}
