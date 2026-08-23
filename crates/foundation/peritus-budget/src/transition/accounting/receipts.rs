//! Exact construction of unprivileged budget receipts.

use crate::{BudgetAmounts, BudgetOperation, BudgetReceipt, BudgetReceiptKind};
use peritus_types::{BudgetId, BudgetReservationId, Sha256Digest};
use vstd::prelude::*;

verus! {

pub(in crate::transition) const fn receipt(
    operation: BudgetOperation,
    kind: BudgetReceiptKind,
    budget_id: BudgetId,
) -> (result: BudgetReceipt)
    ensures
        result.spec_operation() == operation,
        result.spec_kind() == kind,
        crate::identity_model::budget_ids_equal(result.spec_budget_id(), budget_id),
        result.spec_reservation_id().is_none(),
        result.spec_charged().spec_is_zero(),
        result.spec_released().spec_is_zero(),
        result.spec_reported().is_none(),
        result.spec_evidence_digest().is_none(),
{
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
) -> (result: BudgetReceipt)
    ensures
        result.spec_operation() == operation,
        result.spec_kind() == kind,
        crate::identity_model::budget_ids_equal(result.spec_budget_id(), budget_id),
        crate::state::optional_reservation_ids_equal(
            result.spec_reservation_id(),
            Some(reservation_id),
        ),
        result.spec_charged().spec_is_zero(),
        result.spec_released().spec_is_zero(),
        result.spec_reported().is_none(),
        crate::invariant::optional_digests_equal(
            result.spec_evidence_digest(),
            Some(evidence_digest),
        ),
{
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

} // verus!
