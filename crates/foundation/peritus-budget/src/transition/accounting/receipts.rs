//! Exact construction of unprivileged budget receipts.

use crate::{BudgetAmounts, BudgetOperation, BudgetReceipt, BudgetReceiptKind};
use peritus_types::{BudgetId, BudgetReservationId, Sha256Digest};

pub(in crate::transition) const fn receipt(
    operation: BudgetOperation,
    kind: BudgetReceiptKind,
    budget_id: BudgetId,
) -> BudgetReceipt {
    BudgetReceipt::new(
        operation,
        kind,
        budget_id,
        None,
        BudgetAmounts::zero(),
        BudgetAmounts::zero(),
        None,
        None,
    )
}
pub(in crate::transition) const fn bound_receipt(
    operation: BudgetOperation,
    kind: BudgetReceiptKind,
    budget_id: BudgetId,
    reservation_id: BudgetReservationId,
    evidence_digest: Sha256Digest,
) -> BudgetReceipt {
    BudgetReceipt::new(
        operation,
        kind,
        budget_id,
        Some(reservation_id),
        BudgetAmounts::zero(),
        BudgetAmounts::zero(),
        None,
        Some(evidence_digest),
    )
}
