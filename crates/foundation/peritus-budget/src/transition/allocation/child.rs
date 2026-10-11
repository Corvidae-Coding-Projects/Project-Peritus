//! Exact child-account allocation and idempotent replay.

use super::super::accounting::{find_account, receipt, require_open_lineage};
use crate::{
    BudgetAccountPhase, BudgetAmounts, BudgetError, BudgetErrorKind, BudgetLedger, BudgetOperation,
    BudgetReceipt, BudgetReceiptKind, ChildBudgetRequest,
};

pub(in crate::transition) fn allocate_child(
    ledger: &mut BudgetLedger,
    request: ChildBudgetRequest,
) -> Result<BudgetReceipt, BudgetError> {
    let existing_account = find_account(ledger, request.verified_child_id());
    match existing_account {
        Some(existing_index) => {
            let existing = &ledger.accounts[existing_index];
            if crate::identity_model::parent_matches_id(
                existing.parent_id,
                request.verified_parent_id(),
            ) && crate::identity_model::revision_equal(
                existing.revision,
                request.verified_revision(),
            ) && existing.limits.amounts().equals(request.verified_limits().amounts())
            {
                let replay = receipt(
                    BudgetOperation::AllocateChild,
                    BudgetReceiptKind::Idempotent,
                    request.verified_child_id(),
                );
                return Ok(replay);
            }
            let error = BudgetError::budget(
                BudgetErrorKind::DuplicateBudgetConflict,
                request.verified_child_id(),
            );
            return Err(error);
        }
        None => {}
    }
    let parent_index = match require_open_lineage(ledger, request.verified_parent_id()) {
        Ok(parent_index) => parent_index,
        Err(error) => {
            return Err(error);
        }
    };
    if !crate::identity_model::revision_equal(
        ledger.accounts[parent_index].revision,
        request.verified_revision(),
    ) {
        let error =
            BudgetError::budget(BudgetErrorKind::BindingMismatch, request.verified_parent_id());
        return Err(error);
    }
    let available = match crate::model::available(&ledger.accounts[parent_index]) {
        Ok(available) => available,
        Err(error) => {
            return Err(error);
        }
    };
    let requested = request.verified_limits().amounts();
    if !requested.fits_within(available) {
        let dimensions = requested.exceeding_dimensions(available);
        let error = BudgetError::insufficient(request.verified_parent_id(), dimensions);
        return Err(error);
    }
    ledger.accounts[parent_index].consumed.establish_bounds();
    ledger.accounts[parent_index].operation_reserved.establish_bounds();
    ledger.accounts[parent_index].child_delegated_remaining.establish_bounds();
    ledger.accounts[parent_index].limits.amounts().establish_bounds();
    requested.establish_bounds();
    let delegated =
        match ledger.accounts[parent_index].child_delegated_remaining.checked_add(requested) {
            Ok(delegated) => delegated,
            Err(arithmetic) => {
                return Err(BudgetError::arithmetic(arithmetic));
            }
        };
    ledger.accounts[parent_index].child_delegated_remaining = delegated;
    ledger.accounts.push(crate::state::BudgetAccount {
        id: request.verified_child_id(),
        parent_id: Some(request.verified_parent_id()),
        revision: request.verified_revision(),
        limits: request.verified_limits(),
        consumed: BudgetAmounts::zero(),
        operation_reserved: BudgetAmounts::zero(),
        child_delegated_remaining: BudgetAmounts::zero(),
        phase: BudgetAccountPhase::Open,
    });
    let applied_receipt = BudgetReceipt::new(
        BudgetOperation::AllocateChild,
        BudgetReceiptKind::Applied,
        request.verified_child_id(),
        None,
        BudgetAmounts::zero(),
        BudgetAmounts::zero(),
        None,
        None,
    );
    Ok(applied_receipt)
}
