//! Shared executable projections and proof bindings for accounting folds.

use crate::{BudgetAccountPhase, BudgetAmounts, BudgetLedger};
use peritus_types::BudgetId;
use vstd::prelude::*;

verus! {

pub(super) fn account_at(
    ledger: &BudgetLedger,
    index: usize,
) -> (result: crate::state::BudgetAccount)
    requires (index as int) < ledger.accounts@.len(),
    ensures
        result.id == ledger.accounts[index as int].id,
        result.parent_id == ledger.accounts[index as int].parent_id,
        result.revision == ledger.accounts[index as int].revision,
        result.limits == ledger.accounts[index as int].limits,
        result.consumed == ledger.accounts[index as int].consumed,
        result.operation_reserved == ledger.accounts[index as int].operation_reserved,
        result.child_delegated_remaining
            == ledger.accounts[index as int].child_delegated_remaining,
        result.phase == ledger.accounts[index as int].phase,
{
    ledger.accounts[index]
}

pub(super) fn reservation_at(
    ledger: &BudgetLedger,
    index: usize,
) -> (result: crate::state::ReservationRecord)
    requires (index as int) < ledger.reservations@.len(),
    ensures
        result.request.spec_budget_id()
            == ledger.reservations[index as int].request.spec_budget_id(),
        result.request.spec_reserve()
            == ledger.reservations[index as int].request.spec_reserve(),
        result.observed == ledger.reservations[index as int].observed,
        result.phase == ledger.reservations[index as int].phase,
{
    ledger.reservations[index]
}

pub(super) const fn child_contributes(
    child: &crate::state::BudgetAccount,
    parent_id: BudgetId,
) -> (result: bool)
    ensures
        result == (crate::identity_model::parent_matches(child.parent_id, parent_id)
            && crate::accounting_model::account_not_closed(child.phase)),
{
    let parent_matches = if let Some(actual) = child.parent_id {
        crate::identity_model::budget_id_equal(actual, parent_id)
    } else {
        false
    };
    let not_closed = match child.phase {
        BudgetAccountPhase::Closed => false,
        BudgetAccountPhase::Open
        | BudgetAccountPhase::Draining
        | BudgetAccountPhase::Faulted => true,
    };
    parent_matches && not_closed
}

pub(super) const fn reservation_contributes(
    reservation: &crate::state::ReservationRecord,
    budget_id: BudgetId,
) -> (result: bool)
    ensures
        result == crate::identity_model::budget_ids_equal(
            reservation.request.spec_budget_id(), budget_id,
        ),
{
    crate::identity_model::budget_id_equal(reservation.request.budget_id(), budget_id)
}

pub(super) proof fn bind_child_remaining(
    ledger: &BudgetLedger,
    parent_id: BudgetId,
    index: int,
    remaining: BudgetAmounts,
)
    requires
        0 <= index < ledger.accounts@.len(),
        crate::identity_model::parent_matches(ledger.accounts[index].parent_id, parent_id),
        crate::accounting_model::account_not_closed(ledger.accounts[index].phase),
        BudgetAmounts::spec_difference(
            remaining,
            ledger.accounts[index].limits.spec_amounts(),
            ledger.accounts[index].consumed,
        ),
    ensures
        remaining.spec_get(crate::BudgetDimension::ModelTokens)
            == crate::accounting_model::child_remaining_contribution(
                ledger, parent_id, crate::BudgetDimension::ModelTokens, index,
            ),
        remaining.spec_get(crate::BudgetDimension::ProviderCostMicrounits)
            == crate::accounting_model::child_remaining_contribution(
                ledger, parent_id, crate::BudgetDimension::ProviderCostMicrounits, index,
            ),
        remaining.spec_get(crate::BudgetDimension::ActiveEffectMilliseconds)
            == crate::accounting_model::child_remaining_contribution(
                ledger, parent_id, crate::BudgetDimension::ActiveEffectMilliseconds, index,
            ),
        remaining.spec_get(crate::BudgetDimension::Attempts)
            == crate::accounting_model::child_remaining_contribution(
                ledger, parent_id, crate::BudgetDimension::Attempts, index,
            ),
        remaining.spec_get(crate::BudgetDimension::Retries)
            == crate::accounting_model::child_remaining_contribution(
                ledger, parent_id, crate::BudgetDimension::Retries, index,
            ),
{
}

pub(super) proof fn bind_child_zero(ledger: &BudgetLedger, parent_id: BudgetId, index: int)
    requires
        0 <= index < ledger.accounts@.len(),
        !(crate::identity_model::parent_matches(
                ledger.accounts[index].parent_id,
                parent_id,
            ) && crate::accounting_model::account_not_closed(ledger.accounts[index].phase)),
    ensures
        crate::accounting_model::child_remaining_contribution(
            ledger, parent_id, crate::BudgetDimension::ModelTokens, index,
        ) == 0,
        crate::accounting_model::child_remaining_contribution(
            ledger, parent_id, crate::BudgetDimension::ProviderCostMicrounits, index,
        ) == 0,
        crate::accounting_model::child_remaining_contribution(
            ledger, parent_id, crate::BudgetDimension::ActiveEffectMilliseconds, index,
        ) == 0,
        crate::accounting_model::child_remaining_contribution(
            ledger, parent_id, crate::BudgetDimension::Attempts, index,
        ) == 0,
        crate::accounting_model::child_remaining_contribution(
            ledger, parent_id, crate::BudgetDimension::Retries, index,
        ) == 0,
{
}

pub(super) proof fn bind_reservation_amount(
    ledger: &BudgetLedger,
    budget_id: BudgetId,
    index: int,
    amount: BudgetAmounts,
)
    requires
        0 <= index < ledger.reservations@.len(),
        crate::identity_model::budget_ids_equal(
            ledger.reservations[index].request.spec_budget_id(),
            budget_id,
        ),
        amount.spec_get(crate::BudgetDimension::ModelTokens)
            == crate::accounting_model::record_outstanding(
                ledger.reservations[index], crate::BudgetDimension::ModelTokens,
            ),
        amount.spec_get(crate::BudgetDimension::ProviderCostMicrounits)
            == crate::accounting_model::record_outstanding(
                ledger.reservations[index], crate::BudgetDimension::ProviderCostMicrounits,
            ),
        amount.spec_get(crate::BudgetDimension::ActiveEffectMilliseconds)
            == crate::accounting_model::record_outstanding(
                ledger.reservations[index], crate::BudgetDimension::ActiveEffectMilliseconds,
            ),
        amount.spec_get(crate::BudgetDimension::Attempts)
            == crate::accounting_model::record_outstanding(
                ledger.reservations[index], crate::BudgetDimension::Attempts,
            ),
        amount.spec_get(crate::BudgetDimension::Retries)
            == crate::accounting_model::record_outstanding(
                ledger.reservations[index], crate::BudgetDimension::Retries,
            ),
    ensures
        amount.spec_get(crate::BudgetDimension::ModelTokens)
            == crate::accounting_model::reservation_contribution(
                ledger, budget_id, crate::BudgetDimension::ModelTokens, index,
            ),
        amount.spec_get(crate::BudgetDimension::ProviderCostMicrounits)
            == crate::accounting_model::reservation_contribution(
                ledger, budget_id, crate::BudgetDimension::ProviderCostMicrounits, index,
            ),
        amount.spec_get(crate::BudgetDimension::ActiveEffectMilliseconds)
            == crate::accounting_model::reservation_contribution(
                ledger, budget_id, crate::BudgetDimension::ActiveEffectMilliseconds, index,
            ),
        amount.spec_get(crate::BudgetDimension::Attempts)
            == crate::accounting_model::reservation_contribution(
                ledger, budget_id, crate::BudgetDimension::Attempts, index,
            ),
        amount.spec_get(crate::BudgetDimension::Retries)
            == crate::accounting_model::reservation_contribution(
                ledger, budget_id, crate::BudgetDimension::Retries, index,
            ),
{
}

pub(super) proof fn bind_reservation_zero(
    ledger: &BudgetLedger,
    budget_id: BudgetId,
    index: int,
)
    requires
        0 <= index < ledger.reservations@.len(),
        !crate::identity_model::budget_ids_equal(
            ledger.reservations[index].request.spec_budget_id(),
            budget_id,
        ),
    ensures
        crate::accounting_model::reservation_contribution(
            ledger, budget_id, crate::BudgetDimension::ModelTokens, index,
        ) == 0,
        crate::accounting_model::reservation_contribution(
            ledger, budget_id, crate::BudgetDimension::ProviderCostMicrounits, index,
        ) == 0,
        crate::accounting_model::reservation_contribution(
            ledger, budget_id, crate::BudgetDimension::ActiveEffectMilliseconds, index,
        ) == 0,
        crate::accounting_model::reservation_contribution(
            ledger, budget_id, crate::BudgetDimension::Attempts, index,
        ) == 0,
        crate::accounting_model::reservation_contribution(
            ledger, budget_id, crate::BudgetDimension::Retries, index,
        ) == 0,
{
}

} // verus!
