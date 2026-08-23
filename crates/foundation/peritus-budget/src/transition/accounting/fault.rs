//! Ancestor fault propagation for ambiguous or exceeded reservations.

use crate::{BudgetAccountPhase, BudgetError, BudgetLedger};
use peritus_types::BudgetId;
use vstd::prelude::*;

#[cfg(verus_only)]
use super::fault_safety::{
    fault_lineage_safe, fault_lineage_safe_fuel, local_fault_preserves_parent_safety,
};

verus! {

pub(in crate::transition) fn fault_lineage(
    ledger: &mut BudgetLedger,
    budget_id: BudgetId,
) -> (result: Result<(), BudgetError>)
    requires fault_lineage_safe(old(ledger), budget_id),
    ensures
        crate::identity_model::budget_ids_equal(old(ledger).root_id, final(ledger).root_id),
        final(ledger).accounts@.len() == old(ledger).accounts@.len(),
        final(ledger).reservations@ == old(ledger).reservations@,
        result.is_ok(),
        result.is_ok() ==> crate::reachability::lineage_fault_exact(
            old(ledger),
            final(ledger),
            budget_id,
        ),
{
    let ghost before = *ledger;
    let account_count = ledger.accounts.len();
    let result = fault_lineage_inner(ledger, budget_id, account_count);
    proof {
        if result.is_ok() {
            assert(crate::reachability::lineage_fault_exact(&before, ledger, budget_id));
        }
    }
    result
}

fn fault_lineage_inner(
    ledger: &mut BudgetLedger,
    current_id: BudgetId,
    remaining_steps: usize,
) -> (result: Result<(), BudgetError>)
    requires remaining_steps <= old(ledger).accounts@.len(),
        fault_lineage_safe_fuel(
            old(ledger),
            current_id,
            remaining_steps as nat,
        ),
    ensures
        crate::identity_model::budget_ids_equal(old(ledger).root_id, final(ledger).root_id),
        final(ledger).accounts@.len() == old(ledger).accounts@.len(),
        final(ledger).reservations@ == old(ledger).reservations@,
        result.is_ok(),
        result.is_ok() ==> crate::reachability::lineage_fault_fuel_exact(
            old(ledger).accounts@,
            final(ledger).accounts@,
            current_id,
            remaining_steps as nat,
        ),
    decreases remaining_steps,
{
    if remaining_steps == 0 {
        assert(false);
        return Err(crate::model::corrupt(current_id));
    }
    let ghost before = *ledger;
    let ghost before_accounts = ledger.accounts@;
    let index = match super::find_account(ledger, current_id) {
        Some(index) => index,
        None => {
            assert(false);
            return Err(crate::model::corrupt(current_id));
        }
    };
    let parent_id = ledger.accounts[index].parent_id;
    assert(crate::reachability::account_at_guard(
        &before,
        current_id,
        index as int,
    ));
    ledger.accounts[index].phase = BudgetAccountPhase::Faulted;
    proof {
        assert(ledger.accounts@ == before_accounts.update(
            index as int,
            ledger.accounts[index as int],
        ));
        assert(crate::reachability::faulted_account_exact(
            before_accounts[index as int],
            ledger.accounts[index as int],
        ));
    }
    if let Some(parent_id) = parent_id {
        assert(fault_lineage_safe_fuel(
            &before,
            parent_id,
            (remaining_steps - 1) as nat,
        ));
        proof {
            local_fault_preserves_parent_safety(
                &before,
                ledger,
                parent_id,
                (remaining_steps - 1) as nat,
                index as int,
            );
        }
        fault_lineage_inner(ledger, parent_id, remaining_steps - 1)?;
    }
    assert(crate::reachability::lineage_fault_fuel_exact(
        before_accounts,
        ledger.accounts@,
        current_id,
        remaining_steps as nat,
    ));
    Ok(())
}

} // verus!
