//! Branch-ordered allocation and begin rejection predicates.

#[cfg(verus_only)]
use crate::{BudgetError, BudgetErrorKind, BudgetLedger};
use vstd::prelude::*;

verus! {

pub(crate) open spec fn first_non_open_account(
    ledger: &BudgetLedger,
    index: int,
    error: BudgetError,
) -> bool
    decreases index,
{
    if 0 <= index < ledger.accounts@.len() {
        if ledger.accounts[index].phase != crate::BudgetAccountPhase::Open {
            super::exact_budget_error(
                error,
                BudgetErrorKind::AccountNotOpen,
                ledger.accounts[index].id,
            )
        } else {
            match ledger.accounts[index].parent_id {
                None => false,
                Some(parent_id) => exists |parent: int| #![auto]
                    0 <= parent < index
                        && super::super::guards::account_at(ledger, parent_id, parent)
                        && first_non_open_account(ledger, parent, error),
            }
        }
    } else {
        false
    }
}

pub(crate) open spec fn lineage_rejection(
    ledger: &BudgetLedger,
    budget_id: peritus_types::BudgetId,
    error: BudgetError,
) -> bool {
    (super::no_account(ledger, budget_id)
        && super::exact_budget_error(error, BudgetErrorKind::UnknownBudget, budget_id))
        || exists |index: int| #![auto]
            super::super::guards::account_at(ledger, budget_id, index)
                && first_non_open_account(ledger, index, error)
}

pub(crate) open spec fn allocation_rejection(
    ledger: &BudgetLedger,
    request: crate::ChildBudgetRequest,
    error: BudgetError,
) -> bool {
    if exists |index: int| #![auto]
        super::super::guards::account_at(ledger, request.spec_child_id(), index)
    {
        (exists |index: int| #![auto]
            super::super::guards::account_at(ledger, request.spec_child_id(), index)
                && (!crate::identity_model::parent_matches(
                        ledger.accounts[index].parent_id,
                        request.spec_parent_id(),
                    )
                    || !crate::identity_model::revisions_equal(
                        ledger.accounts[index].revision,
                        request.spec_revision(),
                    )
                    || !ledger.accounts[index].limits.spec_amounts().spec_equal(
                        request.spec_limits().spec_amounts(),
                    )))
            && super::exact_budget_error(
                error,
                BudgetErrorKind::DuplicateBudgetConflict,
                request.spec_child_id(),
            )
    } else if !super::super::guards::lineage_is_open(ledger, request.spec_parent_id()) {
        lineage_rejection(ledger, request.spec_parent_id(), error)
    } else {
        exists |parent: int| #![auto]
            super::super::guards::account_at(ledger, request.spec_parent_id(), parent)
                && allocation_after_lineage_rejection(ledger, request, parent, error)
    }
}

pub(crate) open spec fn allocation_after_lineage_rejection(
    ledger: &BudgetLedger,
    request: crate::ChildBudgetRequest,
    parent: int,
    error: BudgetError,
) -> bool {
    if !crate::identity_model::revisions_equal(
        ledger.accounts[parent].revision,
        request.spec_revision(),
    ) {
        super::exact_budget_error(
            error,
            BudgetErrorKind::BindingMismatch,
            request.spec_parent_id(),
        )
    } else {
        exists |available: crate::BudgetAmounts| #![auto]
            crate::model::available_is_exact(ledger.accounts[parent], available)
                && !request.spec_limits().spec_amounts().spec_le(available)
                && super::exact_insufficient_error(
                    error,
                    request.spec_parent_id(),
                    request.spec_limits().spec_amounts(),
                    available,
                )
    }
}

pub(crate) open spec fn begin_rejection(
    ledger: &BudgetLedger,
    request: crate::BudgetRequest,
    error: BudgetError,
) -> bool {
    if exists |index: int| #![auto]
        super::super::guards::reservation_at(ledger, request.spec_reservation_id(), index)
    {
        (exists |index: int| #![auto]
            super::super::guards::reservation_at(ledger, request.spec_reservation_id(), index)
                && !crate::refinement_model::requests_equal(
                    ledger.reservations[index].request,
                    request,
                ))
            && super::exact_reservation_error(
                error,
                BudgetErrorKind::DuplicateReservationConflict,
                request.spec_reservation_id(),
            )
    } else {
        begin_after_identity_rejection(ledger, request, error)
    }
}

pub(crate) open spec fn begin_after_identity_rejection(
    ledger: &BudgetLedger,
    request: crate::BudgetRequest,
    error: BudgetError,
) -> bool {
    if crate::BudgetAmounts::spec_addition_overflows(
        request.spec_consume_now(),
        request.spec_reserve(),
    ) {
        match error.spec_arithmetic() {
            Some(arithmetic) => {
                crate::BudgetAmounts::addition_error_exact(
                    arithmetic,
                    request.spec_consume_now(),
                    request.spec_reserve(),
                ) && super::exact_arithmetic_error(error, arithmetic)
            }
            None => false,
        }
    } else if request.spec_consume_now().spec_is_zero() && request.spec_reserve().spec_is_zero() {
        super::exact_reservation_error(
            error,
            BudgetErrorKind::EmptyRequest,
            request.spec_reservation_id(),
        )
    } else if !super::super::guards::lineage_is_open(ledger, request.spec_budget_id()) {
        lineage_rejection(ledger, request.spec_budget_id(), error)
    } else {
        exists |account: int| #![auto]
            super::super::guards::account_at(ledger, request.spec_budget_id(), account)
                && begin_after_lineage_rejection(ledger, request, account, error)
    }
}

pub(crate) open spec fn begin_after_lineage_rejection(
    ledger: &BudgetLedger,
    request: crate::BudgetRequest,
    account: int,
    error: BudgetError,
) -> bool {
    if !crate::identity_model::revisions_equal(
        ledger.accounts[account].revision,
        request.spec_revision(),
    ) {
        super::exact_reservation_error(
            error,
            BudgetErrorKind::BindingMismatch,
            request.spec_reservation_id(),
        )
    } else if !crate::invariant::prior_history_resolved(
        ledger,
        request,
        ledger.reservations@.len() as int,
    ) {
        retry_history_rejection(
            ledger,
            request,
            ledger.reservations@.len() as int,
            error,
        )
    } else if !crate::invariant::attempt_charge_valid(
        request,
        crate::invariant::prior_exact_request(
            ledger,
            request,
            ledger.reservations@.len() as int,
        ),
    ) {
        super::exact_reservation_error(
            error,
            BudgetErrorKind::InvalidAttemptAccounting,
            request.spec_reservation_id(),
        )
    } else {
        exists |requested: crate::BudgetAmounts, available: crate::BudgetAmounts| #![auto]
            crate::BudgetAmounts::spec_sum(
                requested,
                request.spec_consume_now(),
                request.spec_reserve(),
            )
                && crate::model::available_is_exact(ledger.accounts[account], available)
                && !requested.spec_le(available)
                && super::exact_insufficient_error(
                    error,
                    request.spec_budget_id(),
                    requested,
                    available,
                )
    }
}

pub(crate) open spec fn retry_history_rejection(
    ledger: &BudgetLedger,
    request: crate::BudgetRequest,
    end: int,
    error: BudgetError,
) -> bool {
    exists |failing: int| #![auto]
        0 <= failing < end <= ledger.reservations@.len()
            && crate::identity_model::revisions_equal(
                ledger.reservations[failing].request.spec_revision(),
                request.spec_revision(),
            )
            && crate::identity_model::action_ids_equal(
                ledger.reservations[failing].request.spec_action_id(),
                request.spec_action_id(),
            )
            && crate::invariant::prior_history_resolved(ledger, request, failing)
            && if !crate::identity_model::digests_equal(
                ledger.reservations[failing].request.spec_action_digest(),
                request.spec_action_digest(),
            ) {
                super::exact_reservation_error(
                    error,
                    BudgetErrorKind::BindingMismatch,
                    request.spec_reservation_id(),
                )
            } else {
                ledger.reservations[failing].phase.spec_is_live()
                    && super::exact_reservation_error(
                        error,
                        BudgetErrorKind::PriorAttemptUnresolved,
                        request.spec_reservation_id(),
                    )
            }
}

} // verus!
