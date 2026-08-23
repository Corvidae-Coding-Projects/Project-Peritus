//! Exact release of outstanding operation reservations.

use super::{find_account, outstanding_validated};
#[cfg(verus_only)]
use super::{known_release_preserves_charge_safety, lineage_charge_safe};
use crate::{BudgetAmounts, BudgetError, BudgetLedger};
use peritus_types::BudgetId;
use vstd::prelude::*;

verus! {

#[allow(
    dead_code,
    reason = "retained exact checked primitive for transition-local reservation release"
)]
pub(in crate::transition) fn release_operation_reservation(
    ledger: &mut BudgetLedger,
    budget_id: BudgetId,
    amount: BudgetAmounts,
) -> (result: Result<(), BudgetError>)
    ensures
        final(ledger).accounts@.len() == old(ledger).accounts@.len(),
        final(ledger).reservations@ == old(ledger).reservations@,
        result.is_ok() ==> crate::reachability::operation_release_exact(
            old(ledger),
            final(ledger),
            budget_id,
            amount,
        ),
        result.is_err() ==> crate::reachability::infrastructure_error(result.unwrap_err()),
{
    let ghost before = *ledger;
    let index = match find_account(ledger, budget_id) {
        Some(index) => index,
        None => return Err(crate::model::corrupt(budget_id)),
    };
    ledger.accounts[index].operation_reserved = ledger.accounts[index]
        .operation_reserved
        .checked_sub(amount)
        .map_err(BudgetError::arithmetic)?;
    proof {
        assert(ledger.accounts@ == before.accounts@.update(
            index as int,
            ledger.accounts[index as int],
        ));
        assert(crate::reachability::released_account_exact(
            before.accounts[index as int],
            ledger.accounts[index as int],
            amount,
        ));
        assert(crate::reachability::operation_release_exact(
            &before,
            ledger,
            budget_id,
            amount,
        ));
    }
    Ok(())
}

pub(in crate::transition) fn release_operation_reservation_validated(
    ledger: &mut BudgetLedger,
    account_index: usize,
    _budget_id: BudgetId,
    amount: BudgetAmounts,
)
    requires
        (account_index as int) < old(ledger).accounts@.len(),
        crate::identity_model::budget_ids_equal(
            old(ledger).accounts[account_index as int].id,
            _budget_id,
        ),
        amount.spec_le(old(ledger).accounts[account_index as int].operation_reserved),
    ensures
        final(ledger).reservations@ == old(ledger).reservations@,
        final(ledger).accounts@.len() == old(ledger).accounts@.len(),
        crate::identity_model::budget_ids_equal(final(ledger).root_id, old(ledger).root_id),
        crate::reachability::operation_release_exact(
            old(ledger),
            final(ledger),
            _budget_id,
            amount,
        ),
{
    let ghost before = *ledger;
    ledger.accounts[account_index].operation_reserved.establish_bounds();
    amount.establish_bounds();
    let after = match ledger.accounts[account_index]
        .operation_reserved
        .checked_sub(amount)
    {
        Ok(after) => after,
        Err(_arithmetic) => {
            proof {
                match _arithmetic.spec_dimension() {
                    crate::BudgetDimension::ModelTokens => assert(false),
                    crate::BudgetDimension::ProviderCostMicrounits => assert(false),
                    crate::BudgetDimension::ActiveEffectMilliseconds => assert(false),
                    crate::BudgetDimension::Attempts => assert(false),
                    crate::BudgetDimension::Retries => assert(false),
                }
            }
            return;
        }
    };
    ledger.accounts[account_index].operation_reserved = after;
    assert(ledger.accounts@ == before.accounts@.update(
        account_index as int,
        ledger.accounts[account_index as int],
    ));
    assert(crate::reachability::released_account_exact(
        before.accounts[account_index as int],
        ledger.accounts[account_index as int],
        amount,
    ));
}

pub(in crate::transition) fn release_observation_charge(
    ledger: &mut BudgetLedger,
    reservation_index: usize,
    amount: BudgetAmounts,
)
    requires
        crate::model::ledger_well_formed(old(ledger)),
        (reservation_index as int) < old(ledger).reservations@.len(),
        old(ledger).reservations[reservation_index as int].phase.spec_is_live(),
        forall |dimension: crate::BudgetDimension| #![auto]
            amount.spec_get(dimension)
                <= crate::accounting_model::record_outstanding(
                    old(ledger).reservations[reservation_index as int],
                    dimension,
                ),
        lineage_charge_safe(
            old(ledger),
            old(ledger).reservations[reservation_index as int].request.spec_budget_id(),
            amount,
        ),
    ensures
        final(ledger).reservations@ == old(ledger).reservations@,
        final(ledger).accounts@.len() == old(ledger).accounts@.len(),
        crate::identity_model::budget_ids_equal(final(ledger).root_id, old(ledger).root_id),
        crate::reachability::operation_release_exact(
            old(ledger),
            final(ledger),
            old(ledger).reservations[reservation_index as int].request.spec_budget_id(),
            amount,
        ),
        lineage_charge_safe(
            final(ledger),
            old(ledger).reservations[reservation_index as int].request.spec_budget_id(),
            amount,
        ),
{
    let ghost before = *ledger;
    let budget_id = ledger.reservations[reservation_index].request.budget_id();
    let account_index = match find_account(ledger, budget_id) {
        Some(index) => index,
        None => {
            proof {
                assert(crate::invariant::reservation_entry_valid(
                    &before,
                    reservation_index as int,
                ));
                let account = choose |account: int| #![auto]
                    0 <= account < before.accounts@.len()
                        && crate::identity_model::budget_ids_equal(
                            before.reservations[reservation_index as int]
                                .request.spec_budget_id(),
                            before.accounts[account].id,
                        );
                assert(false);
            }
            return;
        }
    };
    assert(crate::invariant::reservation_entry_valid(
        &before,
        reservation_index as int,
    ));
    let _full = outstanding_validated(&ledger.reservations[reservation_index]);
    proof {
        crate::accounting_model::reservation_outstanding_le_account(
            &before,
            reservation_index as int,
            account_index as int,
            _full,
        );
        assert(amount.spec_le(before.accounts[account_index as int].operation_reserved));
    }
    ledger.accounts[account_index].operation_reserved.establish_bounds();
    amount.establish_bounds();
    let after = match ledger.accounts[account_index]
        .operation_reserved
        .checked_sub(amount)
    {
        Ok(after) => after,
        Err(_arithmetic) => {
            proof {
                match _arithmetic.spec_dimension() {
                    crate::BudgetDimension::ModelTokens => assert(false),
                    crate::BudgetDimension::ProviderCostMicrounits => assert(false),
                    crate::BudgetDimension::ActiveEffectMilliseconds => assert(false),
                    crate::BudgetDimension::Attempts => assert(false),
                    crate::BudgetDimension::Retries => assert(false),
                }
            }
            return;
        }
    };
    ledger.accounts[account_index].operation_reserved = after;
    assert(ledger.accounts@ == before.accounts@.update(
        account_index as int,
        ledger.accounts[account_index as int],
    ));
    assert(crate::reachability::released_account_exact(
        before.accounts[account_index as int],
        ledger.accounts[account_index as int],
        amount,
    ));
    proof {
        known_release_preserves_charge_safety(
            &before,
            ledger,
            before.reservations[reservation_index as int].request.spec_budget_id(),
            amount,
            amount,
            account_index as int,
        );
    }
}

pub(in crate::transition) fn release_full_reservation(
    ledger: &mut BudgetLedger,
    reservation_index: usize,
    amount: BudgetAmounts,
)
    requires
        crate::model::ledger_well_formed(old(ledger)),
        (reservation_index as int) < old(ledger).reservations@.len(),
        old(ledger).reservations[reservation_index as int].phase.spec_is_live(),
        BudgetAmounts::spec_difference(
            amount,
            old(ledger).reservations[reservation_index as int].request.spec_reserve(),
            old(ledger).reservations[reservation_index as int].observed,
        ),
    ensures
        final(ledger).reservations@ == old(ledger).reservations@,
        final(ledger).accounts@.len() == old(ledger).accounts@.len(),
        crate::identity_model::budget_ids_equal(final(ledger).root_id, old(ledger).root_id),
        crate::reachability::operation_release_exact(
            old(ledger),
            final(ledger),
            old(ledger).reservations[reservation_index as int].request.spec_budget_id(),
            amount,
        ),
        lineage_charge_safe(
            old(ledger),
            old(ledger).reservations[reservation_index as int].request.spec_budget_id(),
            amount,
        ) ==> lineage_charge_safe(
            final(ledger),
            old(ledger).reservations[reservation_index as int].request.spec_budget_id(),
            amount,
        ),
{
    let ghost before = *ledger;
    let budget_id = ledger.reservations[reservation_index].request.budget_id();
    let account_index = match find_account(ledger, budget_id) {
        Some(index) => index,
        None => {
            proof {
                assert(crate::invariant::ledger_structure_holds(&before));
                assert(crate::invariant::reservation_entry_valid(
                    &before,
                    reservation_index as int,
                ));
                let account = choose |account: int| #![auto]
                    0 <= account < before.accounts@.len()
                        && crate::identity_model::budget_ids_equal(
                            before.reservations[reservation_index as int]
                                .request.spec_budget_id(),
                            before.accounts[account].id,
                        );
                assert(false);
            }
            return;
        }
    };
    proof {
        crate::accounting_model::reservation_outstanding_le_account(
            &before,
            reservation_index as int,
            account_index as int,
            amount,
        );
    }
    ledger.accounts[account_index].operation_reserved.establish_bounds();
    amount.establish_bounds();
    let after = match ledger.accounts[account_index]
        .operation_reserved
        .checked_sub(amount)
    {
        Ok(after) => after,
        Err(_arithmetic) => {
            proof {
                assert(amount.spec_le(
                    before.accounts[account_index as int].operation_reserved,
                ));
                match _arithmetic.spec_dimension() {
                    crate::BudgetDimension::ModelTokens => assert(false),
                    crate::BudgetDimension::ProviderCostMicrounits => assert(false),
                    crate::BudgetDimension::ActiveEffectMilliseconds => assert(false),
                    crate::BudgetDimension::Attempts => assert(false),
                    crate::BudgetDimension::Retries => assert(false),
                }
            }
            return;
        }
    };
    ledger.accounts[account_index].operation_reserved = after;
    assert(ledger.accounts@ == before.accounts@.update(
        account_index as int,
        ledger.accounts[account_index as int],
    ));
    assert(crate::reachability::released_account_exact(
        before.accounts[account_index as int],
        ledger.accounts[account_index as int],
        amount,
    ));
    assert(before.accounts[account_index as int].id
        == ledger.accounts[account_index as int].id);
    assert(before.accounts[account_index as int].parent_id
        == ledger.accounts[account_index as int].parent_id);
    assert(before.accounts[account_index as int].consumed
        == ledger.accounts[account_index as int].consumed);
    assert(before.accounts[account_index as int].child_delegated_remaining
        == ledger.accounts[account_index as int].child_delegated_remaining);
    proof {
        if lineage_charge_safe(
            &before,
            before.reservations[reservation_index as int].request.spec_budget_id(),
            amount,
        ) {
            known_release_preserves_charge_safety(
                &before,
                ledger,
                before.reservations[reservation_index as int].request.spec_budget_id(),
                amount,
                amount,
                account_index as int,
            );
        }
    }
}

} // verus!
