//! Executable bridges from reserved or available capacity to lineage charge safety.

use super::credit::establish_account_credit_safe;
#[cfg(verus_only)]
use super::predicates::lineage_charge_safe;
use crate::{BudgetAmounts, BudgetLedger};
use peritus_types::BudgetId;
use vstd::prelude::*;

verus! {

pub(in crate::transition) fn establish_reserved_charge_safe(
    ledger: &BudgetLedger,
    reservation_index: usize,
    amount: BudgetAmounts,
)
    requires
        crate::model::ledger_well_formed(ledger),
        (reservation_index as int) < ledger.reservations@.len(),
        ledger.reservations[reservation_index as int].phase.spec_is_live(),
        amount.spec_le(ledger.reservations[reservation_index as int].request.spec_reserve()),
        ledger.reservations[reservation_index as int].observed.spec_le(
            ledger.reservations[reservation_index as int].request.spec_reserve(),
        ),
        forall |dimension: crate::BudgetDimension| #![auto]
            amount.spec_get(dimension)
                <= crate::accounting_model::record_outstanding(
                    ledger.reservations[reservation_index as int],
                    dimension,
                ),
    ensures lineage_charge_safe(
        ledger,
        ledger.reservations[reservation_index as int].request.spec_budget_id(),
        amount,
    ),
{
    let budget_id = ledger.reservations[reservation_index].request.budget_id();
    let account_index = match super::super::find_account(ledger, budget_id) {
        Some(index) => index,
        None => {
            proof {
                assert(crate::invariant::reservation_entry_valid(
                    ledger,
                    reservation_index as int,
                ));
                let account = choose |account: int| #![auto]
                    0 <= account < ledger.accounts@.len()
                        && crate::identity_model::budget_ids_equal(
                            ledger.reservations[reservation_index as int]
                                .request.spec_budget_id(),
                            ledger.accounts[account].id,
                        );
                assert(false);
            }
            return;
        }
    };
    let full = super::super::outstanding_validated(&ledger.reservations[reservation_index]);
    proof {
        crate::accounting_model::reservation_outstanding_le_account(
            ledger,
            reservation_index as int,
            account_index as int,
            full,
        );
        assert(amount.spec_le(ledger.accounts[account_index as int].operation_reserved));
    }
    assert(crate::accounting_model::account_not_closed(
        ledger.accounts[account_index as int].phase,
    )) by {
        if ledger.accounts[account_index as int].phase == crate::BudgetAccountPhase::Closed {
            assert(crate::invariant::account_entry_valid(
                ledger,
                account_index as int,
            ));
            assert(crate::invariant::closed_account_has_no_live_work(
                ledger,
                account_index as int,
            ));
            assert(crate::identity_model::budget_ids_equal(
                ledger.reservations[reservation_index as int].request.spec_budget_id(),
                ledger.accounts[account_index as int].id,
            ));
            assert(ledger.reservations[reservation_index as int].phase.spec_is_live());
            assert(false);
        }
    }
    establish_account_credit_safe(
        ledger,
        account_index,
        budget_id,
        amount,
        false,
        false,
        ledger.accounts.len(),
    );
    assert(crate::identity_model::budget_ids_equal(
        ledger.accounts[account_index as int].id,
        ledger.reservations[reservation_index as int].request.spec_budget_id(),
    ));
}

pub(in crate::transition) fn establish_available_charge_safe(
    ledger: &BudgetLedger,
    account_index: usize,
    budget_id: BudgetId,
    amount: BudgetAmounts,
)
    requires
        crate::model::ledger_well_formed(ledger),
        account_index < ledger.accounts@.len(),
        crate::identity_model::budget_ids_equal(
            ledger.accounts[account_index as int].id,
            budget_id,
        ),
        crate::accounting_model::account_not_closed(
            ledger.accounts[account_index as int].phase,
        ),
        crate::reachability::capacity_guard(
            ledger.accounts[account_index as int],
            amount,
        ),
    ensures lineage_charge_safe(ledger, budget_id, amount),
{
    establish_account_credit_safe(
        ledger,
        account_index,
        budget_id,
        amount,
        false,
        true,
        ledger.accounts.len(),
    );
}

pub(in crate::transition) fn establish_observation_charge_safe(
    ledger: &BudgetLedger,
    reservation_index: usize,
    cumulative: BudgetAmounts,
    delta: BudgetAmounts,
)
    requires
        crate::model::ledger_well_formed(ledger),
        (reservation_index as int) < ledger.reservations@.len(),
        ledger.reservations[reservation_index as int].phase.spec_is_live(),
        ledger.reservations[reservation_index as int]
            .observed.spec_le(cumulative),
        cumulative.spec_le(
            ledger.reservations[reservation_index as int].request.spec_reserve(),
        ),
        BudgetAmounts::spec_difference(
            delta,
            cumulative,
            ledger.reservations[reservation_index as int].observed,
        ),
    ensures lineage_charge_safe(
        ledger,
        ledger.reservations[reservation_index as int].request.spec_budget_id(),
        delta,
    ),
{
    let reserve = ledger.reservations[reservation_index].request.reserve();
    let observed = ledger.reservations[reservation_index].observed;
    reserve.establish_bounds();
    observed.establish_bounds();
    cumulative.establish_bounds();
    delta.establish_bounds();
    assert(delta.spec_le(reserve));
    assert forall |dimension: crate::BudgetDimension| #![auto]
        delta.spec_get(dimension)
            <= crate::accounting_model::record_outstanding(
                ledger.reservations[reservation_index as int],
                dimension,
            ) by {
        assert(delta.spec_get(dimension)
            == cumulative.spec_get(dimension) - observed.spec_get(dimension));
        assert(cumulative.spec_get(dimension) <= reserve.spec_get(dimension));
    }
    establish_reserved_charge_safe(ledger, reservation_index, delta);
}

} // verus!
