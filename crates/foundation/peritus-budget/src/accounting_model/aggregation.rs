//! Aggregate matching relations and proofs over derived accounting folds.

use super::child_contribution_le_sum;
#[cfg(verus_only)]
use super::{
    account_not_closed, child_remaining_contribution, child_sum_step, direct_child_remaining_sum,
    direct_operation_reserved_sum, reservation_contribution, reservation_sum_step,
};
use crate::{BudgetDimension, BudgetLedger};
#[cfg(verus_only)]
use peritus_types::BudgetId;
use vstd::prelude::*;

verus! {

pub(crate) fn child_unused_le_parent(
    ledger: &BudgetLedger,
    child: usize,
    parent: usize,
    unused: crate::BudgetAmounts,
)
    requires
        crate::model::ledger_well_formed(ledger),
        child < ledger.accounts@.len(),
        parent < ledger.accounts@.len(),
        crate::identity_model::parent_matches(
            ledger.accounts[child as int].parent_id,
            ledger.accounts[parent as int].id,
        ),
        account_not_closed(ledger.accounts[child as int].phase),
        crate::BudgetAmounts::spec_difference(
            unused,
            ledger.accounts[child as int].limits.spec_amounts(),
            ledger.accounts[child as int].consumed,
        ),
    ensures unused.spec_le(ledger.accounts[parent as int].child_delegated_remaining),
{
    unused.establish_bounds();
    let parent_id = ledger.accounts[parent].id;
    proof {
        assert(crate::invariant::account_entry_valid(ledger, parent as int));
        assert(crate::invariant::exact_derived_accounting(ledger, parent as int));
    }
    child_contribution_le_sum(
        ledger,
        parent_id,
        BudgetDimension::ModelTokens,
        child,
        ledger.accounts.len(),
    );
    child_contribution_le_sum(
        ledger,
        parent_id,
        BudgetDimension::ProviderCostMicrounits,
        child,
        ledger.accounts.len(),
    );
    child_contribution_le_sum(
        ledger,
        parent_id,
        BudgetDimension::ActiveEffectMilliseconds,
        child,
        ledger.accounts.len(),
    );
    child_contribution_le_sum(
        ledger,
        parent_id,
        BudgetDimension::Attempts,
        child,
        ledger.accounts.len(),
    );
    child_contribution_le_sum(
        ledger,
        parent_id,
        BudgetDimension::Retries,
        child,
        ledger.accounts.len(),
    );
}

pub(crate) open spec fn amount_matches_child_sum(
    amount: crate::BudgetAmounts,
    ledger: &BudgetLedger,
    parent_id: BudgetId,
    end: int,
) -> bool {
    amount.spec_get(BudgetDimension::ModelTokens)
            == direct_child_remaining_sum(ledger, parent_id, BudgetDimension::ModelTokens, end)
        && amount.spec_get(BudgetDimension::ProviderCostMicrounits)
            == direct_child_remaining_sum(
                ledger,
                parent_id,
                BudgetDimension::ProviderCostMicrounits,
                end,
            )
        && amount.spec_get(BudgetDimension::ActiveEffectMilliseconds)
            == direct_child_remaining_sum(
                ledger,
                parent_id,
                BudgetDimension::ActiveEffectMilliseconds,
                end,
            )
        && amount.spec_get(BudgetDimension::Attempts)
            == direct_child_remaining_sum(ledger, parent_id, BudgetDimension::Attempts, end)
        && amount.spec_get(BudgetDimension::Retries)
            == direct_child_remaining_sum(ledger, parent_id, BudgetDimension::Retries, end)
}

pub(crate) open spec fn amount_matches_reservation_sum(
    amount: crate::BudgetAmounts,
    ledger: &BudgetLedger,
    budget_id: BudgetId,
    end: int,
) -> bool {
    amount.spec_get(BudgetDimension::ModelTokens)
            == direct_operation_reserved_sum(
                ledger,
                budget_id,
                BudgetDimension::ModelTokens,
                end,
            )
        && amount.spec_get(BudgetDimension::ProviderCostMicrounits)
            == direct_operation_reserved_sum(
                ledger,
                budget_id,
                BudgetDimension::ProviderCostMicrounits,
                end,
            )
        && amount.spec_get(BudgetDimension::ActiveEffectMilliseconds)
            == direct_operation_reserved_sum(
                ledger,
                budget_id,
                BudgetDimension::ActiveEffectMilliseconds,
                end,
            )
        && amount.spec_get(BudgetDimension::Attempts)
            == direct_operation_reserved_sum(ledger, budget_id, BudgetDimension::Attempts, end)
        && amount.spec_get(BudgetDimension::Retries)
            == direct_operation_reserved_sum(ledger, budget_id, BudgetDimension::Retries, end)
}

pub(crate) proof fn advance_child_total(
    ledger: &BudgetLedger,
    parent_id: BudgetId,
    index: int,
    before: crate::BudgetAmounts,
    contribution: crate::BudgetAmounts,
    after: crate::BudgetAmounts,
)
    requires
        0 <= index < ledger.accounts@.len(),
        amount_matches_child_sum(before, ledger, parent_id, index),
        crate::BudgetAmounts::spec_sum(after, before, contribution),
        contribution.spec_get(BudgetDimension::ModelTokens)
            == child_remaining_contribution(
                ledger, parent_id, BudgetDimension::ModelTokens, index,
            ),
        contribution.spec_get(BudgetDimension::ProviderCostMicrounits)
            == child_remaining_contribution(
                ledger, parent_id, BudgetDimension::ProviderCostMicrounits, index,
            ),
        contribution.spec_get(BudgetDimension::ActiveEffectMilliseconds)
            == child_remaining_contribution(
                ledger, parent_id, BudgetDimension::ActiveEffectMilliseconds, index,
            ),
        contribution.spec_get(BudgetDimension::Attempts)
            == child_remaining_contribution(
                ledger, parent_id, BudgetDimension::Attempts, index,
            ),
        contribution.spec_get(BudgetDimension::Retries)
            == child_remaining_contribution(
                ledger, parent_id, BudgetDimension::Retries, index,
            ),
    ensures amount_matches_child_sum(after, ledger, parent_id, index + 1),
{
    child_sum_step(ledger, parent_id, BudgetDimension::ModelTokens, index);
    child_sum_step(ledger, parent_id, BudgetDimension::ProviderCostMicrounits, index);
    child_sum_step(ledger, parent_id, BudgetDimension::ActiveEffectMilliseconds, index);
    child_sum_step(ledger, parent_id, BudgetDimension::Attempts, index);
    child_sum_step(ledger, parent_id, BudgetDimension::Retries, index);
}

pub(crate) proof fn advance_child_zero(
    ledger: &BudgetLedger,
    parent_id: BudgetId,
    index: int,
    total: crate::BudgetAmounts,
)
    requires
        0 <= index < ledger.accounts@.len(),
        amount_matches_child_sum(total, ledger, parent_id, index),
        child_remaining_contribution(
            ledger, parent_id, BudgetDimension::ModelTokens, index,
        ) == 0,
        child_remaining_contribution(
            ledger, parent_id, BudgetDimension::ProviderCostMicrounits, index,
        ) == 0,
        child_remaining_contribution(
            ledger, parent_id, BudgetDimension::ActiveEffectMilliseconds, index,
        ) == 0,
        child_remaining_contribution(
            ledger, parent_id, BudgetDimension::Attempts, index,
        ) == 0,
        child_remaining_contribution(
            ledger, parent_id, BudgetDimension::Retries, index,
        ) == 0,
    ensures amount_matches_child_sum(total, ledger, parent_id, index + 1),
{
    child_sum_step(ledger, parent_id, BudgetDimension::ModelTokens, index);
    child_sum_step(ledger, parent_id, BudgetDimension::ProviderCostMicrounits, index);
    child_sum_step(ledger, parent_id, BudgetDimension::ActiveEffectMilliseconds, index);
    child_sum_step(ledger, parent_id, BudgetDimension::Attempts, index);
    child_sum_step(ledger, parent_id, BudgetDimension::Retries, index);
}

pub(crate) proof fn advance_reservation_total(
    ledger: &BudgetLedger,
    budget_id: BudgetId,
    index: int,
    before: crate::BudgetAmounts,
    contribution: crate::BudgetAmounts,
    after: crate::BudgetAmounts,
)
    requires
        0 <= index < ledger.reservations@.len(),
        amount_matches_reservation_sum(before, ledger, budget_id, index),
        crate::BudgetAmounts::spec_sum(after, before, contribution),
        contribution.spec_get(BudgetDimension::ModelTokens)
            == reservation_contribution(ledger, budget_id, BudgetDimension::ModelTokens, index),
        contribution.spec_get(BudgetDimension::ProviderCostMicrounits)
            == reservation_contribution(
                ledger, budget_id, BudgetDimension::ProviderCostMicrounits, index,
            ),
        contribution.spec_get(BudgetDimension::ActiveEffectMilliseconds)
            == reservation_contribution(
                ledger, budget_id, BudgetDimension::ActiveEffectMilliseconds, index,
            ),
        contribution.spec_get(BudgetDimension::Attempts)
            == reservation_contribution(ledger, budget_id, BudgetDimension::Attempts, index),
        contribution.spec_get(BudgetDimension::Retries)
            == reservation_contribution(ledger, budget_id, BudgetDimension::Retries, index),
    ensures amount_matches_reservation_sum(after, ledger, budget_id, index + 1),
{
    reservation_sum_step(ledger, budget_id, BudgetDimension::ModelTokens, index);
    reservation_sum_step(ledger, budget_id, BudgetDimension::ProviderCostMicrounits, index);
    reservation_sum_step(ledger, budget_id, BudgetDimension::ActiveEffectMilliseconds, index);
    reservation_sum_step(ledger, budget_id, BudgetDimension::Attempts, index);
    reservation_sum_step(ledger, budget_id, BudgetDimension::Retries, index);
}

pub(crate) proof fn advance_reservation_zero(
    ledger: &BudgetLedger,
    budget_id: BudgetId,
    index: int,
    total: crate::BudgetAmounts,
)
    requires
        0 <= index < ledger.reservations@.len(),
        amount_matches_reservation_sum(total, ledger, budget_id, index),
        reservation_contribution(ledger, budget_id, BudgetDimension::ModelTokens, index) == 0,
        reservation_contribution(
            ledger, budget_id, BudgetDimension::ProviderCostMicrounits, index,
        ) == 0,
        reservation_contribution(
            ledger, budget_id, BudgetDimension::ActiveEffectMilliseconds, index,
        ) == 0,
        reservation_contribution(ledger, budget_id, BudgetDimension::Attempts, index) == 0,
        reservation_contribution(ledger, budget_id, BudgetDimension::Retries, index) == 0,
    ensures amount_matches_reservation_sum(total, ledger, budget_id, index + 1),
{
    reservation_sum_step(ledger, budget_id, BudgetDimension::ModelTokens, index);
    reservation_sum_step(ledger, budget_id, BudgetDimension::ProviderCostMicrounits, index);
    reservation_sum_step(ledger, budget_id, BudgetDimension::ActiveEffectMilliseconds, index);
    reservation_sum_step(ledger, budget_id, BudgetDimension::Attempts, index);
    reservation_sum_step(ledger, budget_id, BudgetDimension::Retries, index);
}

} // verus!
