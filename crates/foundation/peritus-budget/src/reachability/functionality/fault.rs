//! Functionality of ancestor fault propagation.

#[cfg(verus_only)]
use crate::BudgetLedger;
#[cfg(verus_only)]
use peritus_types::BudgetId;
use vstd::prelude::*;

verus! {

proof fn faulted_updates_match(
    left_before: Seq<crate::state::BudgetAccount>,
    right_before: Seq<crate::state::BudgetAccount>,
    left_updated: crate::state::BudgetAccount,
    right_updated: crate::state::BudgetAccount,
    index: int,
)
    requires
        super::accounting::account_sequences_equal(left_before, right_before),
        0 <= index < left_before.len(),
        super::super::account_updates::faulted_account(
            left_before[index], left_updated,
        ),
        super::super::account_updates::faulted_account(
            right_before[index], right_updated,
        ),
    ensures super::accounting::account_sequences_equal(
        left_before.update(index, left_updated),
        right_before.update(index, right_updated),
    ),
{
    assert forall |query: int| #![auto]
        0 <= query < left_before.len()
            implies super::super::accounts::account_exactly_equal(
                left_before.update(index, left_updated)[query],
                right_before.update(index, right_updated)[query],
            ) by {
    }
}

proof fn lineage_fault_fuel_functional(
    left_before: Seq<crate::state::BudgetAccount>,
    right_before: Seq<crate::state::BudgetAccount>,
    left_after: Seq<crate::state::BudgetAccount>,
    right_after: Seq<crate::state::BudgetAccount>,
    left_current: BudgetId,
    right_current: BudgetId,
    fuel: nat,
)
    requires
        super::accounting::account_sequences_equal(left_before, right_before),
        super::accounting::account_ids_unique(left_before),
        crate::identity_model::budget_ids_equal(left_current, right_current),
        super::super::account_updates::lineage_fault_fuel(
            left_before, left_after, left_current, fuel,
        ),
        super::super::account_updates::lineage_fault_fuel(
            right_before, right_after, right_current, fuel,
        ),
    ensures
        super::accounting::account_sequences_equal(left_after, right_after),
        super::accounting::account_ids_unique(left_after),
    decreases fuel,
{
    let left_index = choose |index: int| #![auto]
        0 <= index < left_before.len()
            && crate::identity_model::budget_ids_equal(
                left_before[index].id, left_current,
            )
            && exists |updated: crate::state::BudgetAccount| #![auto]
            super::super::account_updates::faulted_account(left_before[index], updated)
            && {
                let intermediate = left_before.update(index, updated);
                match left_before[index].parent_id {
                    None => left_after == intermediate,
                    Some(parent) => super::super::account_updates::lineage_fault_fuel(
                        intermediate, left_after, parent, (fuel - 1) as nat,
                    ),
                }
            };
    let left_updated = choose |updated: crate::state::BudgetAccount| #![auto]
        super::super::account_updates::faulted_account(left_before[left_index], updated)
            && {
                let intermediate = left_before.update(left_index, updated);
                match left_before[left_index].parent_id {
                    None => left_after == intermediate,
                    Some(parent) => super::super::account_updates::lineage_fault_fuel(
                        intermediate, left_after, parent, (fuel - 1) as nat,
                    ),
                }
            };
    let right_index = choose |index: int| #![auto]
        0 <= index < right_before.len()
            && crate::identity_model::budget_ids_equal(
                right_before[index].id, right_current,
            )
            && exists |updated: crate::state::BudgetAccount| #![auto]
            super::super::account_updates::faulted_account(right_before[index], updated)
            && {
                let intermediate = right_before.update(index, updated);
                match right_before[index].parent_id {
                    None => right_after == intermediate,
                    Some(parent) => super::super::account_updates::lineage_fault_fuel(
                        intermediate, right_after, parent, (fuel - 1) as nat,
                    ),
                }
            };
    let right_updated = choose |updated: crate::state::BudgetAccount| #![auto]
        super::super::account_updates::faulted_account(right_before[right_index], updated)
            && {
                let intermediate = right_before.update(right_index, updated);
                match right_before[right_index].parent_id {
                    None => right_after == intermediate,
                    Some(parent) => super::super::account_updates::lineage_fault_fuel(
                        intermediate, right_after, parent, (fuel - 1) as nat,
                    ),
                }
            };
    assert(crate::identity_model::budget_ids_equal(
        left_before[left_index].id, left_before[right_index].id,
    ));
    if left_index < right_index {
        assert(false);
    } else if right_index < left_index {
        crate::identity_model::budget_ids_symmetric(
            left_before[left_index].id, left_before[right_index].id,
        );
        assert(false);
    }
    assert(left_index == right_index);
    let left_intermediate = left_before.update(left_index, left_updated);
    let right_intermediate = right_before.update(right_index, right_updated);
    faulted_updates_match(
        left_before, right_before, left_updated, right_updated, left_index,
    );
    super::accounting::updated_ids_remain_unique(
        left_before, left_updated, left_index,
    );
    match (left_before[left_index].parent_id, right_before[right_index].parent_id) {
        (None, None) => {}
        (Some(left_parent), Some(right_parent)) => {
            lineage_fault_fuel_functional(
                left_intermediate,
                right_intermediate,
                left_after,
                right_after,
                left_parent,
                right_parent,
                (fuel - 1) as nat,
            );
        }
        _ => {
            assert(super::super::accounts::account_exactly_equal(
                left_before[left_index], right_before[right_index],
            ));
            reveal(crate::identity_model::parents_equal);
            assert(false);
        }
    }
}

pub(super) proof fn overrun_functional(
    left_before: &BudgetLedger,
    right_before: &BudgetLedger,
    left_after: &BudgetLedger,
    right_after: &BudgetLedger,
    left_released: &BudgetLedger,
    right_released: &BudgetLedger,
    left_charged: &BudgetLedger,
    right_charged: &BudgetLedger,
    left_budget: BudgetId,
    right_budget: BudgetId,
    left_amount: crate::BudgetAmounts,
    right_amount: crate::BudgetAmounts,
)
    requires
        super::accounting::account_sequences_equal(
            left_before.accounts@, right_before.accounts@,
        ),
        super::accounting::account_ids_unique(left_before.accounts@),
        crate::identity_model::budget_ids_equal(left_budget, right_budget),
        left_amount.spec_equal(right_amount),
        super::super::account_updates::overrun_accounting(
            left_before, left_after, left_released, left_charged,
            left_budget, left_amount,
        ),
        super::super::account_updates::overrun_accounting(
            right_before, right_after, right_released, right_charged,
            right_budget, right_amount,
        ),
    ensures super::accounting::account_sequences_equal(
        left_after.accounts@, right_after.accounts@,
    ),
{
    reveal(super::super::account_updates::overrun_accounting);
    super::release::full_charge_functional(
        left_before, right_before, left_charged, right_charged,
        left_budget, right_budget, left_amount, right_amount,
    );
    reveal(super::super::account_updates::lineage_fault);
    lineage_fault_fuel_functional(
        left_charged.accounts@,
        right_charged.accounts@,
        left_after.accounts@,
        right_after.accounts@,
        left_budget,
        right_budget,
        left_charged.accounts@.len() as nat,
    );
}

} // verus!
