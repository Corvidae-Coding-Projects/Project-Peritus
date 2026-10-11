//! Non-authorizing transition receipts and accepted value-in/value-out plans.

use super::BudgetLedger;
use crate::BudgetAmounts;
use peritus_types::{BudgetId, BudgetReservationId, Sha256Digest};

#[doc = " Stable logical operation named by a receipt."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BudgetOperation {
    #[doc = " Child allocation."]
    AllocateChild,
    #[doc = " Atomic immediate charge and reservation."]
    Begin,
    #[doc = " Held-to-active transition."]
    Activate,
    #[doc = " Cumulative observation reconciliation."]
    ObserveUsage,
    #[doc = " Exact full-ceiling settlement."]
    SettleExact,
    #[doc = " Pre-activation cancellation."]
    CancelHeld,
    #[doc = " Conservative ambiguous finalization."]
    FinalizeAmbiguous,
    #[doc = " Account sealing."]
    Seal,
    #[doc = " Account closure."]
    Close,
}
#[doc = " Stable outcome class of an accepted command."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BudgetReceiptKind {
    #[doc = " The command changed accounting or lifecycle state."]
    Applied,
    #[doc = " An exact replay produced no additional state change."]
    Idempotent,
    #[doc = " Above-ceiling evidence consumed the remaining ceiling and faulted the lineage."]
    OverrunFaulted,
}
#[doc = " Non-authorizing evidence of one accepted logical transition."]
#[doc = ""]
#[doc = " A receipt is neither a durable-commit receipt nor effect authority. In particular, a"]
#[doc = " `CancelHeld` receipt cannot substitute for C0\'s authoritative negative observation required by"]
#[doc = " `REF-C0-B1-COMMIT-ONCE`."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BudgetReceipt {
    operation: BudgetOperation,
    kind: BudgetReceiptKind,
    budget_id: BudgetId,
    reservation_id: Option<BudgetReservationId>,
    charged: BudgetAmounts,
    released: BudgetAmounts,
    reported: Option<BudgetAmounts>,
    evidence_digest: Option<Sha256Digest>,
}
impl BudgetReceipt {
    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn new(
        operation: BudgetOperation,
        kind: BudgetReceiptKind,
        budget_id: BudgetId,
        reservation_id: Option<BudgetReservationId>,
        charged: BudgetAmounts,
        released: BudgetAmounts,
        reported: Option<BudgetAmounts>,
        evidence_digest: Option<Sha256Digest>,
    ) -> Self {
        Self {
            operation,
            kind,
            budget_id,
            reservation_id,
            charged,
            released,
            reported,
            evidence_digest,
        }
    }
    #[doc = " Returns the command operation."]
    #[must_use]
    pub const fn operation(self) -> BudgetOperation {
        self.operation
    }
    #[doc = " Returns whether this applied, replayed, or faulted."]
    #[must_use]
    pub const fn kind(self) -> BudgetReceiptKind {
        self.kind
    }
    #[doc = " Returns the affected account."]
    #[must_use]
    pub const fn budget_id(self) -> BudgetId {
        self.budget_id
    }
    #[doc = " Returns the reservation identity when the operation has one."]
    #[must_use]
    pub const fn reservation_id(self) -> Option<BudgetReservationId> {
        self.reservation_id
    }
    #[doc = " Returns newly authoritative consumption for the affected lineage."]
    #[must_use]
    pub const fn charged(self) -> BudgetAmounts {
        self.charged
    }
    #[doc = " Returns capacity released without reducing consumption."]
    #[must_use]
    pub const fn released(self) -> BudgetAmounts {
        self.released
    }
    #[doc = " Returns the raw representable cumulative report when applicable."]
    #[must_use]
    pub const fn reported(self) -> Option<BudgetAmounts> {
        self.reported
    }
    #[doc = " Returns the correlated evidence digest when applicable."]
    #[must_use]
    pub const fn evidence_digest(self) -> Option<Sha256Digest> {
        self.evidence_digest
    }
}
#[doc = " Accepted value-in/value-out reducer result."]
#[doc = ""]
#[doc = " This is a pure logical plan, not proof that its successor was durably committed and not"]
#[doc = " authority to dispatch or cancel an effect. C0 owns the exact compare-and-commit boundary,"]
#[doc = " including the external non-activation observation required before committing `CancelHeld`."]
#[derive(Debug, Eq, PartialEq)]
pub struct BudgetTransition {
    ledger: BudgetLedger,
    receipt: BudgetReceipt,
}
impl BudgetTransition {
    pub(crate) const fn new(ledger: BudgetLedger, receipt: BudgetReceipt) -> Self {
        Self { ledger, receipt }
    }
    #[doc = " Borrows the exact next ledger."]
    #[must_use]
    pub const fn ledger(&self) -> &BudgetLedger {
        &self.ledger
    }
    #[doc = " Returns the stable logical receipt."]
    #[must_use]
    pub const fn receipt(&self) -> BudgetReceipt {
        self.receipt
    }
    #[doc = " Returns a checked snapshot of one successor account."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns the budget reducer\'s typed corruption or unknown-account error."]
    pub fn account_snapshot(
        &self,
        budget_id: BudgetId,
    ) -> Result<crate::BudgetSnapshot, crate::BudgetError> {
        crate::transition::snapshot_account(&self.ledger, budget_id)
    }
    #[doc = " Returns a checked snapshot of one successor reservation."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns the budget reducer\'s typed corruption or unknown-reservation error."]
    pub fn reservation_snapshot(
        &self,
        reservation_id: BudgetReservationId,
    ) -> Result<crate::ReservationSnapshot, crate::BudgetError> {
        crate::transition::snapshot_reservation(&self.ledger, reservation_id)
    }
    #[doc = " Consumes the transition and returns its exact next ledger."]
    #[must_use]
    pub fn into_ledger(self) -> BudgetLedger {
        self.ledger
    }
    #[doc = " Consumes the transition into its next ledger and receipt."]
    #[must_use]
    pub fn into_parts(self) -> (BudgetLedger, BudgetReceipt) {
        (self.ledger, self.receipt)
    }
}
