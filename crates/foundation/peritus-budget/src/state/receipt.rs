//! Non-authorizing transition receipts and accepted value-in/value-out plans.

use super::BudgetLedger;
use crate::BudgetAmounts;
use peritus_types::{BudgetId, BudgetReservationId, Sha256Digest};
use vstd::prelude::*;

verus! {

/// Stable logical operation named by a receipt.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BudgetOperation {
    /// Child allocation.
    AllocateChild,
    /// Atomic immediate charge and reservation.
    Begin,
    /// Held-to-active transition.
    Activate,
    /// Cumulative observation reconciliation.
    ObserveUsage,
    /// Exact full-ceiling settlement.
    SettleExact,
    /// Pre-activation cancellation.
    CancelHeld,
    /// Conservative ambiguous finalization.
    FinalizeAmbiguous,
    /// Account sealing.
    Seal,
    /// Account closure.
    Close,
}

/// Stable outcome class of an accepted command.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BudgetReceiptKind {
    /// The command changed accounting or lifecycle state.
    Applied,
    /// An exact replay produced no additional state change.
    Idempotent,
    /// Above-ceiling evidence consumed the remaining ceiling and faulted the lineage.
    OverrunFaulted,
}

/// Non-authorizing evidence of one accepted logical transition.
///
/// A receipt is neither a durable-commit receipt nor effect authority. In particular, a
/// `CancelHeld` receipt cannot substitute for C0's authoritative negative observation required by
/// `REF-C0-B1-COMMIT-ONCE`.
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
    pub(crate) closed spec fn spec_operation(&self) -> BudgetOperation { self.operation }
    pub(crate) closed spec fn spec_kind(&self) -> BudgetReceiptKind { self.kind }
    pub(crate) closed spec fn spec_budget_id(&self) -> BudgetId { self.budget_id }
    pub(crate) closed spec fn spec_reservation_id(&self) -> Option<BudgetReservationId> {
        self.reservation_id
    }
    pub(crate) closed spec fn spec_charged(&self) -> BudgetAmounts { self.charged }
    pub(crate) closed spec fn spec_released(&self) -> BudgetAmounts { self.released }
    pub(crate) closed spec fn spec_reported(&self) -> Option<BudgetAmounts> { self.reported }
    pub(crate) closed spec fn spec_evidence_digest(&self) -> Option<Sha256Digest> {
        self.evidence_digest
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate) const fn new(
        operation: BudgetOperation,
        kind: BudgetReceiptKind,
        budget_id: BudgetId,
        reservation_id: Option<BudgetReservationId>,
        charged: BudgetAmounts,
        released: BudgetAmounts,
        reported: Option<BudgetAmounts>,
        evidence_digest: Option<Sha256Digest>,
    ) -> (result: Self)
        ensures
            result.spec_operation() == operation,
            result.spec_kind() == kind,
            crate::identity_model::budget_ids_equal(result.spec_budget_id(), budget_id),
            optional_reservation_ids_equal(result.spec_reservation_id(), reservation_id),
            result.spec_charged().spec_equal(charged),
            result.spec_released().spec_equal(released),
            crate::invariant::optional_amounts_equal(result.spec_reported(), reported),
            crate::invariant::optional_digests_equal(
                result.spec_evidence_digest(),
                evidence_digest,
            ),
    {
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

    /// Returns the command operation.
    #[must_use]
    pub const fn operation(self) -> BudgetOperation { self.operation }
    /// Returns whether this applied, replayed, or faulted.
    #[must_use]
    pub const fn kind(self) -> BudgetReceiptKind { self.kind }
    /// Returns the affected account.
    #[must_use]
    pub const fn budget_id(self) -> BudgetId { self.budget_id }
    /// Returns the reservation identity when the operation has one.
    #[must_use]
    pub const fn reservation_id(self) -> Option<BudgetReservationId> { self.reservation_id }
    /// Returns newly authoritative consumption for the affected lineage.
    #[must_use]
    pub const fn charged(self) -> BudgetAmounts { self.charged }
    /// Returns capacity released without reducing consumption.
    #[must_use]
    pub const fn released(self) -> BudgetAmounts { self.released }
    /// Returns the raw representable cumulative report when applicable.
    #[must_use]
    pub const fn reported(self) -> Option<BudgetAmounts> { self.reported }
    /// Returns the correlated evidence digest when applicable.
    #[must_use]
    pub const fn evidence_digest(self) -> Option<Sha256Digest> { self.evidence_digest }
}

pub(crate) open spec fn optional_reservation_ids_equal(
    left: Option<BudgetReservationId>,
    right: Option<BudgetReservationId>,
) -> bool {
    match (left, right) {
        (Some(left_id), Some(right_id)) => {
            crate::identity_model::reservation_ids_equal(left_id, right_id)
        }
        (None, None) => true,
        _ => false,
    }
}

/// Accepted value-in/value-out reducer result.
///
/// This is a pure logical plan, not proof that its successor was durably committed and not
/// authority to dispatch or cancel an effect. C0 owns the exact compare-and-commit boundary,
/// including the external non-activation observation required before committing `CancelHeld`.
#[derive(Debug, Eq, PartialEq)]
pub struct BudgetTransition {
    ledger: BudgetLedger,
    receipt: BudgetReceipt,
}

impl BudgetTransition {
    pub(crate) closed spec fn spec_ledger(&self) -> BudgetLedger { self.ledger }
    pub(crate) closed spec fn spec_receipt(&self) -> BudgetReceipt { self.receipt }

    pub(in crate) const fn new(
        ledger: BudgetLedger,
        receipt: BudgetReceipt,
    ) -> (result: Self)
        ensures
            result.spec_ledger() == ledger,
            result.spec_receipt() == receipt,
    {
        Self { ledger, receipt }
    }

    /// Borrows the exact next ledger.
    #[must_use]
    pub const fn ledger(&self) -> &BudgetLedger { &self.ledger }
    /// Returns the stable logical receipt.
    #[must_use]
    pub const fn receipt(&self) -> BudgetReceipt { self.receipt }
    /// Returns a checked snapshot of one successor account.
    ///
    /// # Errors
    ///
    /// Returns the budget reducer's typed corruption or unknown-account error.
    pub fn account_snapshot(&self, budget_id: BudgetId) -> Result<crate::BudgetSnapshot, crate::BudgetError> {
        crate::transition::snapshot_account(&self.ledger, budget_id)
    }
    /// Returns a checked snapshot of one successor reservation.
    ///
    /// # Errors
    ///
    /// Returns the budget reducer's typed corruption or unknown-reservation error.
    pub fn reservation_snapshot(
        &self,
        reservation_id: BudgetReservationId,
    ) -> Result<crate::ReservationSnapshot, crate::BudgetError> {
        crate::transition::snapshot_reservation(&self.ledger, reservation_id)
    }
    /// Consumes the transition and returns its exact next ledger.
    #[must_use]
    pub fn into_ledger(self) -> BudgetLedger { self.ledger }
    /// Consumes the transition into its next ledger and receipt.
    #[must_use]
    pub fn into_parts(self) -> (BudgetLedger, BudgetReceipt) { (self.ledger, self.receipt) }
}

} // verus!
