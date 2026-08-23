//! Recursive specification of when a charge is safe across an account lineage.

use crate::{BudgetAmounts, BudgetLedger};
use peritus_types::BudgetId;
use vstd::prelude::*;

verus! {

pub(in crate::transition) open spec fn lineage_charge_safe_fuel(
    ledger: &BudgetLedger,
    current_id: BudgetId,
    amount: BudgetAmounts,
    delegated_child: bool,
    fuel: nat,
) -> bool
    decreases fuel,
{
    fuel > 0
        && (exists |index: int| #![auto]
            crate::reachability::account_at_guard(ledger, current_id, index))
        && (forall |index: int| #![auto]
            crate::reachability::account_at_guard(ledger, current_id, index)
                ==> (!delegated_child
                        || amount.spec_le(ledger.accounts[index].child_delegated_remaining))
                    && !BudgetAmounts::spec_addition_overflows(
                        ledger.accounts[index].consumed,
                        amount,
                    )
                    && match ledger.accounts[index].parent_id {
                        Some(parent_id) => {
                            lineage_charge_safe_fuel(
                                ledger,
                                parent_id,
                                amount,
                                true,
                                (fuel - 1) as nat,
                            ) && (forall |parent_index: int| #![auto]
                                crate::reachability::account_at_guard(
                                    ledger,
                                    parent_id,
                                    parent_index,
                                ) ==> parent_index < index)
                        }
                        None => true,
                    })
}

pub(in crate::transition) open spec fn lineage_charge_safe(
    ledger: &BudgetLedger,
    budget_id: BudgetId,
    amount: BudgetAmounts,
) -> bool {
    lineage_charge_safe_fuel(
        ledger,
        budget_id,
        amount,
        false,
        ledger.accounts@.len() as nat,
    )
}

} // verus!
