//! Product-facing projection of a verified terminal run settlement.

use peritus_run_settlement::{CandidateStage, RunDisposition, RunSettlement};

use super::{ProductRunMessageError, ProductRunPhase, ProductRunSnapshot};

/// Product protocol name for the automated candidate qualification stage.
pub type ProductCandidateQualification = CandidateStage;

/// Complete user-facing snapshot paired with its verified terminal settlement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductRunSettlementSnapshot {
    snapshot: ProductRunSnapshot,
    settlement: RunSettlement,
}

impl ProductRunSettlementSnapshot {
    /// Pairs one product snapshot with the verified settlement for the same exact candidate.
    ///
    /// # Errors
    ///
    /// Rejects run/workspace identity mismatches, missing or extra deliverables, qualification
    /// mismatches, or an accepted disposition whose product phase is not complete.
    pub fn new(
        snapshot: ProductRunSnapshot,
        settlement: RunSettlement,
    ) -> Result<Self, ProductRunMessageError> {
        let candidate = settlement.checkpoint();
        let deliverable = snapshot.deliverable();
        if candidate.is_some() != deliverable.is_some() {
            return Err(ProductRunMessageError::InvalidSettlement);
        }
        if let Some(checkpoint) = candidate
            && (checkpoint.identity().run_id() != snapshot.run_id()
                || checkpoint.identity().workspace_id() != snapshot.workspace_id()
                || deliverable.is_none_or(|value| value.qualification() != checkpoint.stage()))
        {
            return Err(ProductRunMessageError::InvalidSettlement);
        }
        if settlement.disposition() == RunDisposition::Accepted
            && (snapshot.phase() != ProductRunPhase::Complete
                || deliverable
                    .is_none_or(|value| value.qualification() != CandidateStage::Qualified))
        {
            return Err(ProductRunMessageError::InvalidSettlement);
        }
        Ok(Self { snapshot, settlement })
    }

    /// Human-readable product snapshot.
    #[must_use]
    pub const fn snapshot(&self) -> &ProductRunSnapshot {
        &self.snapshot
    }

    /// Verified terminal disposition and candidate evidence.
    #[must_use]
    pub const fn settlement(&self) -> &RunSettlement {
        &self.settlement
    }
}
