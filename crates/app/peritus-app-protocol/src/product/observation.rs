//! Mixed active and settled run observations without inventing terminal evidence.

use super::{ProductRunMessageError, ProductRunSettlementSnapshot, ProductRunSnapshot};
use peritus_run_settlement::{CandidateStage, RunSettlement};

/// One run's current snapshot and its optional exact terminal settlement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductRunObservation {
    snapshot: ProductRunSnapshot,
    settlement: Option<RunSettlement>,
}

impl ProductRunObservation {
    /// Checks terminal evidence independently for each run in a mixed collection.
    ///
    /// # Errors
    /// Rejects mismatched settlement or an unqualified candidate without its evidence.
    pub fn new(
        snapshot: ProductRunSnapshot,
        settlement: Option<RunSettlement>,
    ) -> Result<Self, ProductRunMessageError> {
        if let Some(settlement) = settlement {
            ProductRunSettlementSnapshot::new(snapshot.clone(), settlement)?;
        } else if snapshot
            .deliverable()
            .is_some_and(|value| value.qualification() != CandidateStage::Qualified)
        {
            return Err(ProductRunMessageError::InvalidSettlement);
        }
        Ok(Self { snapshot, settlement })
    }

    /// Borrows current run state, including the exact candidate handoff when present.
    #[must_use]
    pub const fn snapshot(&self) -> &ProductRunSnapshot {
        &self.snapshot
    }

    /// Returns terminal evidence when it exists; active runs have none.
    #[must_use]
    pub const fn settlement(&self) -> Option<RunSettlement> {
        self.settlement
    }
}
