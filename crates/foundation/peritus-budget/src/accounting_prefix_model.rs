//! Prefix bounds for executable reconstruction of derived accounting totals.

use crate::{BudgetDimension, BudgetLedger};
use vstd::prelude::*;

verus! {

fn child_contribution_nonnegative(
    ledger: &BudgetLedger,
    parent: usize,
    dimension: BudgetDimension,
    index: usize,
)
    requires
        parent < ledger.accounts@.len(),
        index < ledger.accounts@.len(),
    ensures
        crate::model::ledger_well_formed(ledger) ==> (
            crate::accounting_model::child_remaining_contribution(
                ledger,
                ledger.accounts[parent as int].id,
                dimension,
                index as int,
            ) >= 0
        ),
{
    ledger.accounts[index].limits.amounts().establish_bounds();
    ledger.accounts[index].consumed.establish_bounds();
    ledger.accounts[index].operation_reserved.establish_bounds();
    ledger.accounts[index].child_delegated_remaining.establish_bounds();
    proof {
        if crate::model::ledger_well_formed(ledger) {
            assert(crate::model::ledger_conserves(ledger));
            assert(crate::model::account_conserves(ledger.accounts[index as int]));
            match dimension {
                BudgetDimension::ModelTokens => {}
                BudgetDimension::ProviderCostMicrounits => {}
                BudgetDimension::ActiveEffectMilliseconds => {}
                BudgetDimension::Attempts => {}
                BudgetDimension::Retries => {}
            }
        }
    }
}

fn child_prefix_dimension_le_account(
    ledger: &BudgetLedger,
    parent: usize,
    dimension: BudgetDimension,
    end: usize,
)
    requires
        parent < ledger.accounts@.len(),
        end <= ledger.accounts@.len(),
    ensures
        crate::model::ledger_well_formed(ledger) ==> (
            crate::accounting_model::direct_child_remaining_sum(
                ledger,
                ledger.accounts[parent as int].id,
                dimension,
                end as int,
            ) <= ledger.accounts[parent as int]
                .child_delegated_remaining
                .spec_get(dimension)
        ),
    decreases ledger.accounts.len() - end,
{
    proof {
        if crate::model::ledger_well_formed(ledger) {
            assert(crate::invariant::ledger_structure_holds(ledger));
            assert(crate::invariant::ledger_account_structure_holds(ledger));
            assert(crate::invariant::account_entry_valid(ledger, parent as int));
            assert(crate::invariant::exact_derived_accounting(ledger, parent as int));
        }
    }
    if end < ledger.accounts.len() {
        child_prefix_dimension_le_account(ledger, parent, dimension, (end + 1) as usize);
        child_contribution_nonnegative(ledger, parent, dimension, end);
        proof {
            crate::accounting_model::child_sum_step(
                ledger,
                ledger.accounts[parent as int].id,
                dimension,
                end as int,
            );
        }
    }
}

pub(crate) fn child_prefix_le_account(
    ledger: &BudgetLedger,
    parent: usize,
    end: usize,
)
    requires
        parent < ledger.accounts@.len(),
        end <= ledger.accounts@.len(),
    ensures
        crate::model::ledger_well_formed(ledger) ==> (
        crate::accounting_model::direct_child_remaining_sum(
            ledger,
            ledger.accounts[parent as int].id,
            BudgetDimension::ModelTokens,
            end as int,
        ) <= ledger.accounts[parent as int]
            .child_delegated_remaining
            .spec_get(BudgetDimension::ModelTokens)
        &&
        crate::accounting_model::direct_child_remaining_sum(
            ledger,
            ledger.accounts[parent as int].id,
            BudgetDimension::ProviderCostMicrounits,
            end as int,
        ) <= ledger.accounts[parent as int]
            .child_delegated_remaining
            .spec_get(BudgetDimension::ProviderCostMicrounits)
        &&
        crate::accounting_model::direct_child_remaining_sum(
            ledger,
            ledger.accounts[parent as int].id,
            BudgetDimension::ActiveEffectMilliseconds,
            end as int,
        ) <= ledger.accounts[parent as int]
            .child_delegated_remaining
            .spec_get(BudgetDimension::ActiveEffectMilliseconds)
        &&
        crate::accounting_model::direct_child_remaining_sum(
            ledger,
            ledger.accounts[parent as int].id,
            BudgetDimension::Attempts,
            end as int,
        ) <= ledger.accounts[parent as int]
            .child_delegated_remaining
            .spec_get(BudgetDimension::Attempts)
        &&
        crate::accounting_model::direct_child_remaining_sum(
            ledger,
            ledger.accounts[parent as int].id,
            BudgetDimension::Retries,
            end as int,
        ) <= ledger.accounts[parent as int]
            .child_delegated_remaining
            .spec_get(BudgetDimension::Retries)),
{
    child_prefix_dimension_le_account(ledger, parent, BudgetDimension::ModelTokens, end);
    child_prefix_dimension_le_account(
        ledger,
        parent,
        BudgetDimension::ProviderCostMicrounits,
        end,
    );
    child_prefix_dimension_le_account(
        ledger,
        parent,
        BudgetDimension::ActiveEffectMilliseconds,
        end,
    );
    child_prefix_dimension_le_account(ledger, parent, BudgetDimension::Attempts, end);
    child_prefix_dimension_le_account(ledger, parent, BudgetDimension::Retries, end);
}

fn reservation_contribution_nonnegative(
    ledger: &BudgetLedger,
    account: usize,
    dimension: BudgetDimension,
    index: usize,
)
    requires
        account < ledger.accounts@.len(),
        index < ledger.reservations@.len(),
    ensures
        crate::model::ledger_well_formed(ledger) ==> (
            crate::accounting_model::reservation_contribution(
                ledger,
                ledger.accounts[account as int].id,
                dimension,
                index as int,
            ) >= 0
        ),
{
    ledger.reservations[index].request.reserve().establish_bounds();
    ledger.reservations[index].observed.establish_bounds();
    proof {
        if crate::model::ledger_well_formed(ledger) {
            assert(crate::invariant::ledger_structure_holds(ledger));
            assert(crate::invariant::ledger_reservation_structure_holds(ledger));
            assert(crate::invariant::reservation_entry_valid(ledger, index as int));
            match dimension {
                BudgetDimension::ModelTokens => {}
                BudgetDimension::ProviderCostMicrounits => {}
                BudgetDimension::ActiveEffectMilliseconds => {}
                BudgetDimension::Attempts => {}
                BudgetDimension::Retries => {}
            }
        }
    }
}

fn reservation_prefix_dimension_le_account(
    ledger: &BudgetLedger,
    account: usize,
    dimension: BudgetDimension,
    end: usize,
)
    requires
        account < ledger.accounts@.len(),
        end <= ledger.reservations@.len(),
    ensures
        crate::model::ledger_well_formed(ledger) ==> (
        crate::accounting_model::direct_operation_reserved_sum(
            ledger,
            ledger.accounts[account as int].id,
            dimension,
            end as int,
        ) <= ledger.accounts[account as int].operation_reserved.spec_get(dimension)),
    decreases ledger.reservations.len() - end,
{
    proof {
        if crate::model::ledger_well_formed(ledger) {
            assert(crate::invariant::ledger_structure_holds(ledger));
            assert(crate::invariant::ledger_account_structure_holds(ledger));
            assert(crate::invariant::account_entry_valid(ledger, account as int));
            assert(crate::invariant::exact_derived_accounting(ledger, account as int));
        }
    }
    if end < ledger.reservations.len() {
        reservation_prefix_dimension_le_account(ledger, account, dimension, (end + 1) as usize);
        reservation_contribution_nonnegative(ledger, account, dimension, end);
        proof {
            crate::accounting_model::reservation_sum_step(
                ledger,
                ledger.accounts[account as int].id,
                dimension,
                end as int,
            );
        }
    }
}

pub(crate) fn reservation_prefix_le_account(
    ledger: &BudgetLedger,
    account: usize,
    end: usize,
)
    requires
        account < ledger.accounts@.len(),
        end <= ledger.reservations@.len(),
    ensures
        crate::model::ledger_well_formed(ledger) ==> (
        crate::accounting_model::direct_operation_reserved_sum(
            ledger,
            ledger.accounts[account as int].id,
            BudgetDimension::ModelTokens,
            end as int,
        ) <= ledger.accounts[account as int]
            .operation_reserved
            .spec_get(BudgetDimension::ModelTokens)
        &&
        crate::accounting_model::direct_operation_reserved_sum(
            ledger,
            ledger.accounts[account as int].id,
            BudgetDimension::ProviderCostMicrounits,
            end as int,
        ) <= ledger.accounts[account as int]
            .operation_reserved
            .spec_get(BudgetDimension::ProviderCostMicrounits)
        &&
        crate::accounting_model::direct_operation_reserved_sum(
            ledger,
            ledger.accounts[account as int].id,
            BudgetDimension::ActiveEffectMilliseconds,
            end as int,
        ) <= ledger.accounts[account as int]
            .operation_reserved
            .spec_get(BudgetDimension::ActiveEffectMilliseconds)
        &&
        crate::accounting_model::direct_operation_reserved_sum(
            ledger,
            ledger.accounts[account as int].id,
            BudgetDimension::Attempts,
            end as int,
        ) <= ledger.accounts[account as int]
            .operation_reserved
            .spec_get(BudgetDimension::Attempts)
        &&
        crate::accounting_model::direct_operation_reserved_sum(
            ledger,
            ledger.accounts[account as int].id,
            BudgetDimension::Retries,
            end as int,
        ) <= ledger.accounts[account as int]
            .operation_reserved
            .spec_get(BudgetDimension::Retries)),
{
    reservation_prefix_dimension_le_account(
        ledger,
        account,
        BudgetDimension::ModelTokens,
        end,
    );
    reservation_prefix_dimension_le_account(
        ledger,
        account,
        BudgetDimension::ProviderCostMicrounits,
        end,
    );
    reservation_prefix_dimension_le_account(
        ledger,
        account,
        BudgetDimension::ActiveEffectMilliseconds,
        end,
    );
    reservation_prefix_dimension_le_account(
        ledger,
        account,
        BudgetDimension::Attempts,
        end,
    );
    reservation_prefix_dimension_le_account(
        ledger,
        account,
        BudgetDimension::Retries,
        end,
    );
}

} // verus!
