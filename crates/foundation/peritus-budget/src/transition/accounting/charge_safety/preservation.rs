//! Proofs that account updates preserving charge-relevant shape retain lineage safety.

use super::predicates::{lineage_charge_safe, lineage_charge_safe_fuel};
use crate::{BudgetAmounts, BudgetLedger};
use peritus_types::BudgetId;
use vstd::prelude::*;

verus! {

pub(in crate::transition) open spec fn charge_shape_equal(
    before: &BudgetLedger,
    after: &BudgetLedger,
) -> bool {
    before.accounts@.len() == after.accounts@.len()
        && forall |index: int| #![auto]
            0 <= index < before.accounts@.len()
                ==> before.accounts[index].id == after.accounts[index].id
                    && before.accounts[index].parent_id == after.accounts[index].parent_id
                    && before.accounts[index].consumed == after.accounts[index].consumed
                    && before.accounts[index].child_delegated_remaining
                        == after.accounts[index].child_delegated_remaining
}

pub(in crate::transition) proof fn known_release_preserves_charge_safety(
    before: &BudgetLedger,
    after: &BudgetLedger,
    budget_id: BudgetId,
    released: BudgetAmounts,
    charge: BudgetAmounts,
    updated_index: int,
)
    requires
        0 <= updated_index < before.accounts@.len(),
        crate::identity_model::budget_ids_equal(
            before.accounts[updated_index].id,
            budget_id,
        ),
        crate::reachability::released_account_exact(
            before.accounts[updated_index],
            after.accounts[updated_index],
            released,
        ),
        before.accounts[updated_index].id == after.accounts[updated_index].id,
        before.accounts[updated_index].parent_id == after.accounts[updated_index].parent_id,
        before.accounts[updated_index].consumed == after.accounts[updated_index].consumed,
        before.accounts[updated_index].child_delegated_remaining
            == after.accounts[updated_index].child_delegated_remaining,
        after.accounts@ == before.accounts@.update(
            updated_index,
            after.accounts[updated_index],
        ),
        lineage_charge_safe(before, budget_id, charge),
    ensures lineage_charge_safe(after, budget_id, charge),
{
    assert(charge_shape_equal(before, after)) by {
        assert forall |index: int| #![auto]
            0 <= index < before.accounts@.len() implies
                before.accounts[index].id == after.accounts[index].id
                && before.accounts[index].parent_id == after.accounts[index].parent_id
                && before.accounts[index].consumed == after.accounts[index].consumed
                && before.accounts[index].child_delegated_remaining
                    == after.accounts[index].child_delegated_remaining by {
            if index == updated_index {
                assert(crate::reachability::released_account_exact(
                    before.accounts[index],
                    after.accounts[index],
                    released,
                ));
            } else {
                assert(after.accounts[index] == before.accounts[index]);
            }
        }
    }
    charge_shape_preserves_safe_fuel(
        before,
        after,
        budget_id,
        charge,
        false,
        before.accounts@.len() as nat,
    );
}

proof fn charge_shape_preserves_safe_fuel(
    before: &BudgetLedger,
    after: &BudgetLedger,
    current_id: BudgetId,
    amount: BudgetAmounts,
    delegated_child: bool,
    fuel: nat,
)
    requires
        charge_shape_equal(before, after),
        lineage_charge_safe_fuel(
            before,
            current_id,
            amount,
            delegated_child,
            fuel,
        ),
    ensures lineage_charge_safe_fuel(
        after,
        current_id,
        amount,
        delegated_child,
        fuel,
    ),
    decreases fuel,
{
    assert forall |after_index: int| #![auto]
        crate::reachability::account_at_guard(after, current_id, after_index) implies
            crate::reachability::account_at_guard(before, current_id, after_index) by {
    }
    assert forall |after_index: int| #![auto]
        crate::reachability::account_at_guard(after, current_id, after_index) implies
            (!delegated_child
                    || amount.spec_le(after.accounts[after_index].child_delegated_remaining))
                && !BudgetAmounts::spec_addition_overflows(
                    after.accounts[after_index].consumed,
                    amount,
                )
                && match after.accounts[after_index].parent_id {
                    Some(parent_id) => {
                        lineage_charge_safe_fuel(
                            after,
                            parent_id,
                            amount,
                            true,
                            (fuel - 1) as nat,
                        ) && (forall |parent_index: int| #![auto]
                            crate::reachability::account_at_guard(
                                after,
                                parent_id,
                                parent_index,
                            ) ==> parent_index < after_index)
                    }
                    None => true,
                } by {
        assert(crate::reachability::account_at_guard(before, current_id, after_index));
        match before.accounts[after_index].parent_id {
            Some(parent_id) => {
                charge_shape_preserves_safe_fuel(
                    before,
                    after,
                    parent_id,
                    amount,
                    true,
                    (fuel - 1) as nat,
                );
                assert forall |parent_index: int| #![auto]
                    crate::reachability::account_at_guard(after, parent_id, parent_index)
                        implies parent_index < after_index by {
                    assert(crate::reachability::account_at_guard(
                        before,
                        parent_id,
                        parent_index,
                    ));
                }
            }
            None => {}
        }
    }
    let before_index = choose |before_index: int| #![auto]
        crate::reachability::account_at_guard(before, current_id, before_index);
    assert(crate::reachability::account_at_guard(after, current_id, before_index));
}

pub(in crate::transition) proof fn later_account_update_preserves_safe_fuel(
    before: &BudgetLedger,
    after: &BudgetLedger,
    current_id: BudgetId,
    amount: BudgetAmounts,
    delegated_child: bool,
    fuel: nat,
    changed_index: int,
)
    requires
        0 <= changed_index < before.accounts@.len(),
        before.accounts@.len() == after.accounts@.len(),
        after.accounts@ == before.accounts@.update(
            changed_index,
            after.accounts[changed_index],
        ),
        before.accounts[changed_index].id == after.accounts[changed_index].id,
        lineage_charge_safe_fuel(before, current_id, amount, delegated_child, fuel),
        forall |index: int| #![auto]
            crate::reachability::account_at_guard(before, current_id, index)
                ==> index < changed_index,
    ensures lineage_charge_safe_fuel(after, current_id, amount, delegated_child, fuel),
    decreases fuel,
{
    assert forall |after_index: int| #![auto]
        crate::reachability::account_at_guard(after, current_id, after_index) implies
            after_index != changed_index
            && crate::reachability::account_at_guard(before, current_id, after_index) by {
        if after_index == changed_index {
            assert(crate::identity_model::budget_ids_equal(
                before.accounts[changed_index].id,
                current_id,
            ));
            assert(crate::reachability::account_at_guard(
                before,
                current_id,
                changed_index,
            ));
            assert(false);
        }
    }
    assert forall |after_index: int| #![auto]
        crate::reachability::account_at_guard(after, current_id, after_index) implies
            (!delegated_child
                    || amount.spec_le(after.accounts[after_index].child_delegated_remaining))
                && !BudgetAmounts::spec_addition_overflows(
                    after.accounts[after_index].consumed,
                    amount,
                )
                && match after.accounts[after_index].parent_id {
                    Some(parent_id) => {
                        lineage_charge_safe_fuel(
                            after,
                            parent_id,
                            amount,
                            true,
                            (fuel - 1) as nat,
                        ) && (forall |parent_index: int| #![auto]
                            crate::reachability::account_at_guard(
                                after,
                                parent_id,
                                parent_index,
                            ) ==> parent_index < after_index)
                    }
                    None => true,
                } by {
        assert(crate::reachability::account_at_guard(
            before,
            current_id,
            after_index,
        ));
        assert(after.accounts[after_index] == before.accounts[after_index]);
        match before.accounts[after_index].parent_id {
            Some(parent_id) => {
                assert forall |parent_index: int| #![auto]
                    crate::reachability::account_at_guard(before, parent_id, parent_index)
                        implies parent_index < after_index by {
                }
                later_account_update_preserves_safe_fuel(
                    before,
                    after,
                    parent_id,
                    amount,
                    true,
                    (fuel - 1) as nat,
                    changed_index,
                );
                assert forall |parent_index: int| #![auto]
                    crate::reachability::account_at_guard(after, parent_id, parent_index)
                        implies parent_index < after_index by {
                    if parent_index == changed_index {
                        assert(crate::identity_model::budget_ids_equal(
                            before.accounts[changed_index].id,
                            parent_id,
                        ));
                    }
                    assert(crate::reachability::account_at_guard(before, parent_id, parent_index));
                }
            }
            None => {}
        }
    }
    let before_index = choose |before_index: int| #![auto]
        crate::reachability::account_at_guard(before, current_id, before_index);
    assert(before_index != changed_index);
    assert(crate::reachability::account_at_guard(
        after,
        current_id,
        before_index,
    ));
}

} // verus!
