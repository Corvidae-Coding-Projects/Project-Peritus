//! Exact proof that the public root constructor creates the unique initial shape.

#[cfg(verus_only)]
use crate::{BudgetAccountPhase, BudgetDimension, BudgetLedger};
use vstd::prelude::*;

verus! {

proof fn initial_dimension_is_derived(
    ledger: &BudgetLedger,
    dimension: BudgetDimension,
)
    requires
        ledger.accounts@.len() == 1,
        ledger.reservations@.len() == 0,
        ledger.accounts[0].parent_id.is_none(),
        ledger.accounts[0].child_delegated_remaining.spec_get(dimension) == 0,
        ledger.accounts[0].operation_reserved.spec_get(dimension) == 0,
    ensures
        ledger.accounts[0].child_delegated_remaining.spec_get(dimension)
            == crate::accounting_model::direct_child_remaining_sum(
                ledger,
                ledger.accounts[0].id,
                dimension,
                ledger.accounts@.len() as int,
            ),
        ledger.accounts[0].operation_reserved.spec_get(dimension)
            == crate::accounting_model::direct_operation_reserved_sum(
                ledger,
                ledger.accounts[0].id,
                dimension,
                ledger.reservations@.len() as int,
            ),
{
    assert(crate::accounting_model::direct_child_remaining_sum(
        ledger,
        ledger.accounts[0].id,
        dimension,
        0,
    ) == 0);
    assert(crate::accounting_model::direct_operation_reserved_sum(
        ledger,
        ledger.accounts[0].id,
        dimension,
        0,
    ) == 0);
    assert(!crate::identity_model::parent_matches(
        ledger.accounts[0].parent_id,
        ledger.accounts[0].id,
    ));
    assert(crate::accounting_model::child_remaining_contribution(
        ledger,
        ledger.accounts[0].id,
        dimension,
        0,
    ) == 0);
    crate::accounting_model::child_sum_step(
        ledger,
        ledger.accounts[0].id,
        dimension,
        0,
    );
}

pub(crate) proof fn single_root_is_well_formed(ledger: &BudgetLedger)
    requires
        ledger.accounts@.len() == 1,
        ledger.reservations@.len() == 0,
        crate::identity_model::budget_ids_equal(ledger.accounts[0].id, ledger.root_id),
        ledger.accounts[0].parent_id.is_none(),
        ledger.accounts[0].consumed.spec_is_zero(),
        ledger.accounts[0].operation_reserved.spec_is_zero(),
        ledger.accounts[0].child_delegated_remaining.spec_is_zero(),
        ledger.accounts[0].phase == BudgetAccountPhase::Open,
        ledger.accounts[0].limits.spec_amounts().spec_get(BudgetDimension::ModelTokens) >= 0,
        ledger.accounts[0].limits.spec_amounts().spec_get(
            BudgetDimension::ProviderCostMicrounits,
        ) >= 0,
        ledger.accounts[0].limits.spec_amounts().spec_get(
            BudgetDimension::ActiveEffectMilliseconds,
        ) >= 0,
        ledger.accounts[0].limits.spec_amounts().spec_get(BudgetDimension::Attempts) >= 0,
        ledger.accounts[0].limits.spec_amounts().spec_get(BudgetDimension::Retries) >= 0,
    ensures crate::model::ledger_well_formed(ledger),
{
    initial_dimension_is_derived(ledger, BudgetDimension::ModelTokens);
    initial_dimension_is_derived(ledger, BudgetDimension::ProviderCostMicrounits);
    initial_dimension_is_derived(ledger, BudgetDimension::ActiveEffectMilliseconds);
    initial_dimension_is_derived(ledger, BudgetDimension::Attempts);
    initial_dimension_is_derived(ledger, BudgetDimension::Retries);
    assert(crate::invariant::exact_derived_accounting(ledger, 0));
    assert(crate::invariant::account_unique_before(ledger, 0));
    assert(crate::invariant::account_entry_valid(ledger, 0));
    assert(crate::invariant::ledger_account_structure_holds(ledger));
    assert(crate::invariant::ledger_reservation_structure_holds(ledger));
    assert(crate::invariant::ledger_structure_holds(ledger));
    assert(ledger.accounts[0].consumed.spec_get(BudgetDimension::ModelTokens) == 0);
    assert(ledger.accounts[0].operation_reserved.spec_get(BudgetDimension::ModelTokens) == 0);
    assert(ledger.accounts[0].child_delegated_remaining.spec_get(BudgetDimension::ModelTokens) == 0);
    assert(crate::model::account_balance_holds(
        ledger.accounts[0],
        BudgetDimension::ModelTokens,
    ));
    assert(ledger.accounts[0].consumed.spec_get(BudgetDimension::ProviderCostMicrounits) == 0);
    assert(ledger.accounts[0].operation_reserved.spec_get(BudgetDimension::ProviderCostMicrounits) == 0);
    assert(ledger.accounts[0].child_delegated_remaining.spec_get(BudgetDimension::ProviderCostMicrounits) == 0);
    assert(crate::model::account_balance_holds(
        ledger.accounts[0],
        BudgetDimension::ProviderCostMicrounits,
    ));
    assert(ledger.accounts[0].consumed.spec_get(BudgetDimension::ActiveEffectMilliseconds) == 0);
    assert(ledger.accounts[0].operation_reserved.spec_get(BudgetDimension::ActiveEffectMilliseconds) == 0);
    assert(ledger.accounts[0].child_delegated_remaining.spec_get(BudgetDimension::ActiveEffectMilliseconds) == 0);
    assert(crate::model::account_balance_holds(
        ledger.accounts[0],
        BudgetDimension::ActiveEffectMilliseconds,
    ));
    assert(ledger.accounts[0].consumed.spec_get(BudgetDimension::Attempts) == 0);
    assert(ledger.accounts[0].operation_reserved.spec_get(BudgetDimension::Attempts) == 0);
    assert(ledger.accounts[0].child_delegated_remaining.spec_get(BudgetDimension::Attempts) == 0);
    assert(crate::model::account_balance_holds(
        ledger.accounts[0],
        BudgetDimension::Attempts,
    ));
    assert(ledger.accounts[0].consumed.spec_get(BudgetDimension::Retries) == 0);
    assert(ledger.accounts[0].operation_reserved.spec_get(BudgetDimension::Retries) == 0);
    assert(ledger.accounts[0].child_delegated_remaining.spec_get(BudgetDimension::Retries) == 0);
    assert(crate::model::account_balance_holds(
        ledger.accounts[0],
        BudgetDimension::Retries,
    ));
    assert(crate::model::account_conserves(ledger.accounts[0]));
    assert(crate::model::ledger_conserves(ledger));
}

} // verus!
