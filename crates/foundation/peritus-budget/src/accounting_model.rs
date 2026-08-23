//! Exact specification folds for derived child and reservation accounting.

#![allow(
    unused_variables,
    clippy::only_used_in_recursion,
    clippy::redundant_pub_crate,
    clippy::semicolon_if_nothing_returned,
    reason = "Recursive accounting witnesses are consumed by Verus specifications and erase from ordinary Rust"
)]

mod aggregation;

pub(crate) use self::aggregation::child_unused_le_parent;

#[cfg(verus_only)]
pub(crate) use self::aggregation::{
    advance_child_total, advance_child_zero, advance_reservation_total,
    advance_reservation_zero, amount_matches_child_sum, amount_matches_reservation_sum,
};

#[cfg(verus_only)]
use crate::BudgetAccountPhase;
use crate::{BudgetDimension, BudgetLedger};
use peritus_types::BudgetId;
use vstd::prelude::*;

verus! {

pub(crate) open spec fn child_remaining_contribution(
    ledger: &BudgetLedger,
    parent_id: BudgetId,
    dimension: BudgetDimension,
    index: int,
) -> int {
    let child = ledger.accounts[index];
    if crate::identity_model::parent_matches(child.parent_id, parent_id)
        && account_not_closed(child.phase)
    {
        child.limits.spec_amounts().spec_get(dimension) - child.consumed.spec_get(dimension)
    } else {
        0
    }
}

pub(crate) open spec fn record_outstanding(
    record: crate::state::ReservationRecord,
    dimension: BudgetDimension,
) -> int {
    if record.phase.spec_is_live() {
        record.request.spec_reserve().spec_get(dimension) - record.observed.spec_get(dimension)
    } else {
        0
    }
}

pub(crate) open spec fn reservation_contribution(
    ledger: &BudgetLedger,
    budget_id: BudgetId,
    dimension: BudgetDimension,
    index: int,
) -> int {
    let record = ledger.reservations[index];
    if crate::identity_model::budget_ids_equal(
        record.request.spec_budget_id(),
        budget_id,
    ) && record.phase.spec_is_live() {
        record_outstanding(record, dimension)
    } else {
        0
    }
}

pub(crate) open spec fn account_not_closed(phase: BudgetAccountPhase) -> bool {
    match phase {
        BudgetAccountPhase::Closed => false,
        BudgetAccountPhase::Open
        | BudgetAccountPhase::Draining
        | BudgetAccountPhase::Faulted => true,
    }
}

pub(crate) open spec fn direct_child_remaining_sum(
    ledger: &BudgetLedger,
    parent_id: BudgetId,
    dimension: BudgetDimension,
    end: int,
) -> int
    decreases end,
{
    if end <= 0 {
        0
    } else {
        let index = end - 1;
        direct_child_remaining_sum(ledger, parent_id, dimension, index)
            + child_remaining_contribution(ledger, parent_id, dimension, index)
    }
}

pub(crate) open spec fn direct_operation_reserved_sum(
    ledger: &BudgetLedger,
    budget_id: BudgetId,
    dimension: BudgetDimension,
    end: int,
) -> int
    decreases end,
{
    if end <= 0 {
        0
    } else {
        let index = end - 1;
        direct_operation_reserved_sum(ledger, budget_id, dimension, index)
            + reservation_contribution(ledger, budget_id, dimension, index)
    }
}

pub(crate) proof fn child_sum_step(
    ledger: &BudgetLedger,
    parent_id: BudgetId,
    dimension: BudgetDimension,
    index: int,
)
    requires 0 <= index < ledger.accounts@.len(),
    ensures
        direct_child_remaining_sum(ledger, parent_id, dimension, index + 1)
            == direct_child_remaining_sum(ledger, parent_id, dimension, index)
                + child_remaining_contribution(ledger, parent_id, dimension, index),
{
}

pub(crate) proof fn reservation_sum_step(
    ledger: &BudgetLedger,
    budget_id: BudgetId,
    dimension: BudgetDimension,
    index: int,
)
    requires 0 <= index < ledger.reservations@.len(),
    ensures
        direct_operation_reserved_sum(ledger, budget_id, dimension, index + 1)
            == direct_operation_reserved_sum(ledger, budget_id, dimension, index)
                + reservation_contribution(ledger, budget_id, dimension, index),
{
}

pub(crate) proof fn reservation_sum_nonnegative(
    ledger: &BudgetLedger,
    budget_id: BudgetId,
    dimension: BudgetDimension,
    end: int,
)
    requires
        crate::model::ledger_well_formed(ledger),
        0 <= end <= ledger.reservations@.len(),
    ensures direct_operation_reserved_sum(ledger, budget_id, dimension, end) >= 0,
    decreases end,
{
    if end > 0 {
        reservation_sum_nonnegative(ledger, budget_id, dimension, end - 1);
        assert(crate::invariant::reservation_entry_valid(ledger, end - 1));
        assert(ledger.reservations[end - 1].observed.spec_le(
            ledger.reservations[end - 1].request.spec_reserve(),
        ));
        assert(reservation_contribution(ledger, budget_id, dimension, end - 1) >= 0);
        reservation_sum_step(ledger, budget_id, dimension, end - 1);
    }
}

pub(crate) proof fn reservation_contribution_le_sum(
    ledger: &BudgetLedger,
    budget_id: BudgetId,
    dimension: BudgetDimension,
    index: int,
    end: int,
)
    requires
        crate::model::ledger_well_formed(ledger),
        0 <= index < end <= ledger.reservations@.len(),
    ensures
        reservation_contribution(ledger, budget_id, dimension, index)
            <= direct_operation_reserved_sum(ledger, budget_id, dimension, end),
    decreases end,
{
    let last = end - 1;
    reservation_sum_step(ledger, budget_id, dimension, last);
    assert(crate::invariant::reservation_entry_valid(ledger, last));
    assert(ledger.reservations[last].observed.spec_le(
        ledger.reservations[last].request.spec_reserve(),
    ));
    assert(reservation_contribution(ledger, budget_id, dimension, last) >= 0);
    if index == last {
        reservation_sum_nonnegative(ledger, budget_id, dimension, last);
    } else {
        reservation_contribution_le_sum(ledger, budget_id, dimension, index, last);
    }
}

pub(crate) proof fn reservation_outstanding_le_account(
    ledger: &BudgetLedger,
    reservation: int,
    account: int,
    amount: crate::BudgetAmounts,
)
    requires
        crate::model::ledger_well_formed(ledger),
        0 <= reservation < ledger.reservations@.len(),
        0 <= account < ledger.accounts@.len(),
        ledger.reservations[reservation].phase.spec_is_live(),
        crate::identity_model::budget_ids_equal(
            ledger.reservations[reservation].request.spec_budget_id(),
            ledger.accounts[account].id,
        ),
        crate::BudgetAmounts::spec_difference(
            amount,
            ledger.reservations[reservation].request.spec_reserve(),
            ledger.reservations[reservation].observed,
        ),
    ensures amount.spec_le(ledger.accounts[account].operation_reserved),
{
    assert(crate::invariant::account_entry_valid(ledger, account));
    assert(crate::invariant::exact_derived_accounting(ledger, account));
    reservation_contribution_le_sum(
        ledger,
        ledger.accounts[account].id,
        BudgetDimension::ModelTokens,
        reservation,
        ledger.reservations@.len() as int,
    );
    reservation_contribution_le_sum(
        ledger,
        ledger.accounts[account].id,
        BudgetDimension::ProviderCostMicrounits,
        reservation,
        ledger.reservations@.len() as int,
    );
    reservation_contribution_le_sum(
        ledger,
        ledger.accounts[account].id,
        BudgetDimension::ActiveEffectMilliseconds,
        reservation,
        ledger.reservations@.len() as int,
    );
    reservation_contribution_le_sum(
        ledger,
        ledger.accounts[account].id,
        BudgetDimension::Attempts,
        reservation,
        ledger.reservations@.len() as int,
    );
    reservation_contribution_le_sum(
        ledger,
        ledger.accounts[account].id,
        BudgetDimension::Retries,
        reservation,
        ledger.reservations@.len() as int,
    );
}

pub(crate) fn child_sum_nonnegative(
    ledger: &BudgetLedger,
    parent_id: BudgetId,
    dimension: BudgetDimension,
    end: usize,
)
    requires
        crate::model::ledger_well_formed(ledger),
        end <= ledger.accounts@.len(),
    ensures direct_child_remaining_sum(ledger, parent_id, dimension, end as int) >= 0,
    decreases end,
{
    if end > 0 {
        child_sum_nonnegative(ledger, parent_id, dimension, end - 1);
        ledger.accounts[end - 1].limits.amounts().establish_bounds();
        ledger.accounts[end - 1].consumed.establish_bounds();
        ledger.accounts[end - 1].operation_reserved.establish_bounds();
        ledger.accounts[end - 1].child_delegated_remaining.establish_bounds();
        proof {
            assert(crate::invariant::account_entry_valid(ledger, end as int - 1));
            assert(crate::model::account_conserves(ledger.accounts[end as int - 1]));
            match dimension {
                BudgetDimension::ModelTokens => assert(child_remaining_contribution(
                    ledger, parent_id, dimension, end as int - 1,
                ) >= 0),
                BudgetDimension::ProviderCostMicrounits => assert(child_remaining_contribution(
                    ledger, parent_id, dimension, end as int - 1,
                ) >= 0),
                BudgetDimension::ActiveEffectMilliseconds => assert(child_remaining_contribution(
                    ledger, parent_id, dimension, end as int - 1,
                ) >= 0),
                BudgetDimension::Attempts => assert(child_remaining_contribution(
                    ledger, parent_id, dimension, end as int - 1,
                ) >= 0),
                BudgetDimension::Retries => assert(child_remaining_contribution(
                    ledger, parent_id, dimension, end as int - 1,
                ) >= 0),
            }
            child_sum_step(ledger, parent_id, dimension, end as int - 1);
        }
    }
}

pub(crate) fn child_contribution_le_sum(
    ledger: &BudgetLedger,
    parent_id: BudgetId,
    dimension: BudgetDimension,
    index: usize,
    end: usize,
)
    requires
        crate::model::ledger_well_formed(ledger),
        index < end <= ledger.accounts@.len(),
    ensures
        child_remaining_contribution(ledger, parent_id, dimension, index as int)
            <= direct_child_remaining_sum(ledger, parent_id, dimension, end as int),
    decreases end,
{
    let last = end - 1;
    ledger.accounts[last].limits.amounts().establish_bounds();
    ledger.accounts[last].consumed.establish_bounds();
    ledger.accounts[last].operation_reserved.establish_bounds();
    ledger.accounts[last].child_delegated_remaining.establish_bounds();
    proof {
        child_sum_step(ledger, parent_id, dimension, last as int);
        assert(crate::model::account_conserves(ledger.accounts[last as int]));
        match dimension {
            BudgetDimension::ModelTokens => assert(child_remaining_contribution(
                ledger, parent_id, dimension, last as int,
            ) >= 0),
            BudgetDimension::ProviderCostMicrounits => assert(child_remaining_contribution(
                ledger, parent_id, dimension, last as int,
            ) >= 0),
            BudgetDimension::ActiveEffectMilliseconds => assert(child_remaining_contribution(
                ledger, parent_id, dimension, last as int,
            ) >= 0),
            BudgetDimension::Attempts => assert(child_remaining_contribution(
                ledger, parent_id, dimension, last as int,
            ) >= 0),
            BudgetDimension::Retries => assert(child_remaining_contribution(
                ledger, parent_id, dimension, last as int,
            ) >= 0),
        }
    }
    if index == last {
        child_sum_nonnegative(ledger, parent_id, dimension, last);
    } else {
        child_contribution_le_sum(ledger, parent_id, dimension, index, last);
    }
}

} // verus!
