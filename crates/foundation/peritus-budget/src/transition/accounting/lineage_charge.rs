//! Recursive exact consumption propagation through one account lineage.

use super::find_account;
#[cfg(verus_only)]
use super::{
    later_account_update_preserves_safe_fuel, lineage_charge_safe, lineage_charge_safe_fuel,
};
use crate::{BudgetAmounts, BudgetError, BudgetLedger};
use peritus_types::BudgetId;
use vstd::prelude::*;

verus! {

pub(in crate::transition) fn charge_lineage(
    ledger: &mut BudgetLedger,
    budget_id: BudgetId,
    amount: BudgetAmounts,
) -> (result: Result<(), BudgetError>)
    requires lineage_charge_safe(old(ledger), budget_id, amount),
    ensures
        crate::identity_model::budget_ids_equal(old(ledger).root_id, final(ledger).root_id),
        final(ledger).accounts@.len() == old(ledger).accounts@.len(),
        final(ledger).reservations@ == old(ledger).reservations@,
        result.is_ok(),
        crate::reachability::lineage_charge_exact(
            old(ledger),
            final(ledger),
            budget_id,
            amount,
        ),
{
    let ghost before = *ledger;
    let account_count = ledger.accounts.len();
    let result = charge_lineage_inner(ledger, budget_id, amount, false, account_count);
    proof {
        if result.is_ok() {
            assert(crate::identity_model::budget_ids_equal(before.root_id, ledger.root_id));
            assert(crate::reachability::lineage_charge_exact(
                &before,
                ledger,
                budget_id,
                amount,
            ));
        }
    }
    result
}

fn charge_lineage_inner(
    ledger: &mut BudgetLedger,
    current_id: BudgetId,
    amount: BudgetAmounts,
    delegated_child: bool,
    remaining_steps: usize,
) -> (result: Result<(), BudgetError>)
    requires
        remaining_steps <= old(ledger).accounts@.len(),
        lineage_charge_safe_fuel(
            old(ledger),
            current_id,
            amount,
            delegated_child,
            remaining_steps as nat,
        ),
    ensures
        crate::identity_model::budget_ids_equal(old(ledger).root_id, final(ledger).root_id),
        final(ledger).accounts@.len() == old(ledger).accounts@.len(),
        final(ledger).reservations@ == old(ledger).reservations@,
        result.is_ok(),
        crate::reachability::lineage_charge_fuel_exact(
            old(ledger).accounts@,
            final(ledger).accounts@,
            current_id,
            amount,
            delegated_child,
            remaining_steps as nat,
        ),
    decreases remaining_steps,
{
    if remaining_steps == 0 {
        assert(false);
        return Err(crate::model::corrupt(current_id));
    }
    let ghost before_accounts = ledger.accounts@;
    let ghost before = *ledger;
    let index = match find_account(ledger, current_id) {
        Some(index) => index,
        None => {
            assert(false);
            return Err(crate::model::corrupt(current_id));
        }
    };
    assert(crate::reachability::account_at_guard(
        &before,
        current_id,
        index as int,
    ));
    assert(!delegated_child
        || amount.spec_le(before.accounts[index as int].child_delegated_remaining));
    assert(!BudgetAmounts::spec_addition_overflows(
        before.accounts[index as int].consumed,
        amount,
    ));
    let parent_id = ledger.accounts[index].parent_id;
    if delegated_child {
        ledger.accounts[index].child_delegated_remaining.establish_bounds();
        amount.establish_bounds();
        let delegated = match ledger.accounts[index]
            .child_delegated_remaining
            .checked_sub(amount)
        {
            Ok(delegated) => delegated,
            Err(arithmetic) => {
                proof {
                    match arithmetic.spec_dimension() {
                        crate::BudgetDimension::ModelTokens => assert(false),
                        crate::BudgetDimension::ProviderCostMicrounits => assert(false),
                        crate::BudgetDimension::ActiveEffectMilliseconds => assert(false),
                        crate::BudgetDimension::Attempts => assert(false),
                        crate::BudgetDimension::Retries => assert(false),
                    }
                }
                return Err(BudgetError::arithmetic(arithmetic));
            }
        };
        ledger.accounts[index].child_delegated_remaining = delegated;
    }
    let consumed = match ledger.accounts[index].consumed.checked_add(amount) {
        Ok(consumed) => consumed,
        Err(arithmetic) => {
            assert(BudgetAmounts::spec_addition_overflows(
                before.accounts[index as int].consumed,
                amount,
            ));
            assert(false);
            return Err(BudgetError::arithmetic(arithmetic));
        }
    };
    ledger.accounts[index].consumed = consumed;
    proof {
        assert(ledger.accounts@ == before_accounts.update(
            index as int,
            ledger.accounts[index as int],
        ));
        assert(crate::reachability::charged_account_exact(
            before_accounts[index as int],
            ledger.accounts[index as int],
            amount,
            delegated_child,
        ));
    }
    let ghost locally_charged_accounts = ledger.accounts@;
    if let Some(parent_id) = parent_id {
        assert(lineage_charge_safe_fuel(
            &before,
            parent_id,
            amount,
            true,
            (remaining_steps - 1) as nat,
        ));
        assert forall |parent_index: int| #![auto]
            crate::reachability::account_at_guard(&before, parent_id, parent_index)
                implies parent_index < index as int by {
        }
        proof {
            later_account_update_preserves_safe_fuel(
                &before,
                ledger,
                parent_id,
                amount,
                true,
                (remaining_steps - 1) as nat,
                index as int,
            );
        }
        charge_lineage_inner(ledger, parent_id, amount, true, remaining_steps - 1)?;
    }
    proof {
    let witness = (index as int, locally_charged_accounts[index as int]);
    assert(before_accounts.update(witness.0, witness.1) == locally_charged_accounts);
    match before_accounts[index as int].parent_id {
        None => assert(ledger.accounts@ == locally_charged_accounts),
        Some(parent_id) => assert(crate::reachability::lineage_charge_fuel_exact(
            locally_charged_accounts,
            ledger.accounts@,
            parent_id,
            amount,
            true,
            (remaining_steps - 1) as nat,
        )),
    }
    assert(exists |candidate: (int, crate::state::BudgetAccount)| #![auto]
        candidate == witness
            && 0 <= candidate.0 < before_accounts.len()
            && crate::identity_model::budget_ids_equal(
                before_accounts[candidate.0].id,
                current_id,
            )
            && crate::reachability::charged_account_exact(
                before_accounts[candidate.0],
                candidate.1,
                amount,
                delegated_child,
            )
            && {
                let intermediate = before_accounts.update(candidate.0, candidate.1);
                match before_accounts[candidate.0].parent_id {
                    None => ledger.accounts@ == intermediate,
                    Some(parent_id) => crate::reachability::lineage_charge_fuel_exact(
                        intermediate,
                        ledger.accounts@,
                        parent_id,
                        amount,
                        true,
                        (remaining_steps - 1) as nat,
                    ),
                }
            });
    assert(crate::reachability::lineage_charge_fuel_exact(
        before_accounts,
        ledger.accounts@,
        current_id,
        amount,
        delegated_child,
        remaining_steps as nat,
    ));
    }
    Ok(())
}

} // verus!
