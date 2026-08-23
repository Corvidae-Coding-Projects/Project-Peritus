//! Executable reachability witnesses for infallible ancestor fault propagation.

use crate::BudgetLedger;
use peritus_types::BudgetId;
use vstd::prelude::*;

verus! {

pub(in crate::transition) open spec fn fault_lineage_safe_fuel(
    ledger: &BudgetLedger,
    current_id: BudgetId,
    fuel: nat,
) -> bool
    decreases fuel,
{
    fuel > 0
        && (exists |index: int| #![auto]
            crate::reachability::account_at_guard(ledger, current_id, index))
        && (forall |index: int| #![auto]
            crate::reachability::account_at_guard(ledger, current_id, index)
                ==> match ledger.accounts[index].parent_id {
                    Some(parent_id) => fault_lineage_safe_fuel(
                        ledger,
                        parent_id,
                        (fuel - 1) as nat,
                    ),
                    None => true,
                })
}

pub(in crate::transition) open spec fn fault_lineage_safe(
    ledger: &BudgetLedger,
    budget_id: BudgetId,
) -> bool {
    fault_lineage_safe_fuel(ledger, budget_id, ledger.accounts@.len() as nat)
}

pub(super) closed spec fn fault_identity_shape(before: &BudgetLedger, after: &BudgetLedger) -> bool {
    before.accounts@.len() == after.accounts@.len()
        && forall |index: int| #![auto]
            0 <= index < before.accounts@.len()
                ==> crate::identity_model::budget_ids_equal(
                        before.accounts[index].id,
                        after.accounts[index].id,
                    )
                    && crate::identity_model::parents_equal(
                        before.accounts[index].parent_id,
                        after.accounts[index].parent_id,
                    )
}

pub(super) proof fn fault_shape_preserves_safe_fuel(
    before: &BudgetLedger,
    after: &BudgetLedger,
    current_id: BudgetId,
    fuel: nat,
)
    requires
        fault_identity_shape(before, after),
        fault_lineage_safe_fuel(before, current_id, fuel),
    ensures fault_lineage_safe_fuel(after, current_id, fuel),
    decreases fuel,
{
    assert forall |index: int| #![auto]
        crate::reachability::account_at_guard(after, current_id, index) implies
            crate::reachability::account_at_guard(before, current_id, index) by {
    }
    assert forall |index: int| #![auto]
        crate::reachability::account_at_guard(after, current_id, index) implies
            match after.accounts[index].parent_id {
                Some(parent_id) => fault_lineage_safe_fuel(
                    after,
                    parent_id,
                    (fuel - 1) as nat,
                ),
                None => true,
            } by {
        assert(crate::reachability::account_at_guard(before, current_id, index));
        match (before.accounts[index].parent_id, after.accounts[index].parent_id) {
            (Some(before_parent), Some(after_parent)) => {
                fault_shape_preserves_safe_fuel(
                    before,
                    after,
                    before_parent,
                    (fuel - 1) as nat,
                );
                fault_safe_equal_id(
                    after,
                    before_parent,
                    after_parent,
                    (fuel - 1) as nat,
                );
            }
            (None, None) => {}
            _ => assert(false),
        }
    }
    let index = choose |index: int| #![auto]
        crate::reachability::account_at_guard(before, current_id, index);
    assert(crate::reachability::account_at_guard(after, current_id, index));
}

proof fn fault_safe_equal_id(
    ledger: &BudgetLedger,
    left: BudgetId,
    right: BudgetId,
    fuel: nat,
)
    requires
        crate::identity_model::budget_ids_equal(left, right),
        fault_lineage_safe_fuel(ledger, left, fuel),
    ensures fault_lineage_safe_fuel(ledger, right, fuel),
{
    assert forall |index: int| #![auto]
        crate::reachability::account_at_guard(ledger, right, index) implies
            crate::reachability::account_at_guard(ledger, left, index) by {
    }
    let index = choose |index: int| #![auto]
        crate::reachability::account_at_guard(ledger, left, index);
    assert(crate::reachability::account_at_guard(ledger, right, index));
}

pub(super) proof fn local_fault_preserves_parent_safety(
    before: &BudgetLedger,
    after: &BudgetLedger,
    parent_id: BudgetId,
    fuel: nat,
    changed_index: int,
)
    requires
        0 <= changed_index < before.accounts@.len(),
        after.accounts@ == before.accounts@.update(
            changed_index,
            after.accounts[changed_index],
        ),
        crate::identity_model::budget_ids_equal(
            before.accounts[changed_index].id,
            after.accounts[changed_index].id,
        ),
        crate::identity_model::parents_equal(
            before.accounts[changed_index].parent_id,
            after.accounts[changed_index].parent_id,
        ),
        fault_lineage_safe_fuel(before, parent_id, fuel),
    ensures fault_lineage_safe_fuel(after, parent_id, fuel),
{
    assert(fault_identity_shape(before, after)) by {
        assert forall |index: int| #![auto]
            0 <= index < before.accounts@.len() implies
                crate::identity_model::budget_ids_equal(
                    before.accounts[index].id,
                    after.accounts[index].id,
                ) && crate::identity_model::parents_equal(
                    before.accounts[index].parent_id,
                    after.accounts[index].parent_id,
                ) by {
            if index != changed_index {
                assert(after.accounts[index] == before.accounts[index]);
            }
        }
    }
    fault_shape_preserves_safe_fuel(before, after, parent_id, fuel);
}

pub(in crate::transition) proof fn identity_stability_preserves_fault_safety(
    before: &BudgetLedger,
    after: &BudgetLedger,
    budget_id: BudgetId,
)
    requires
        fault_lineage_safe(before, budget_id),
        before.accounts@.len() == after.accounts@.len(),
        crate::refinement_model::ledger_identity_stable(before, after),
    ensures fault_lineage_safe(after, budget_id),
{
    assert(fault_identity_shape(before, after)) by {
        assert forall |index: int| #![auto]
            0 <= index < before.accounts@.len() implies
                crate::identity_model::budget_ids_equal(
                    before.accounts[index].id,
                    after.accounts[index].id,
                ) && crate::identity_model::parents_equal(
                    before.accounts[index].parent_id,
                    after.accounts[index].parent_id,
                ) by {
            assert(crate::refinement_model::account_identity_stable(
                before,
                after,
                index,
            ));
        }
    }
    fault_shape_preserves_safe_fuel(
        before,
        after,
        budget_id,
        before.accounts@.len() as nat,
    );
}

pub(in crate::transition) fn establish_fault_lineage_safe(
    ledger: &BudgetLedger,
    index: usize,
    current_id: BudgetId,
    fuel: usize,
)
    requires
        crate::model::ledger_well_formed(ledger),
        index < fuel <= ledger.accounts@.len(),
        crate::identity_model::budget_ids_equal(
            ledger.accounts[index as int].id,
            current_id,
        ),
    ensures fault_lineage_safe_fuel(ledger, current_id, fuel as nat),
    decreases fuel,
{
    match ledger.accounts[index].parent_id {
        None => {}
        Some(parent_id) => {
            let parent_index = match super::find_account(ledger, parent_id) {
                Some(parent_index) => parent_index,
                None => {
                    proof {
                        assert(crate::invariant::account_entry_valid(ledger, index as int));
                        assert(crate::invariant::parent_link_valid(ledger, index as int));
                        assert(false);
                    }
                    return;
                }
            };
            assert(parent_index < index) by {
                assert(crate::invariant::account_entry_valid(ledger, index as int));
                assert(crate::invariant::parent_link_valid(ledger, index as int));
                let linked = choose |linked: int| #![auto]
                    0 <= linked < index
                        && crate::identity_model::parent_matches(
                            ledger.accounts[index as int].parent_id,
                            ledger.accounts[linked].id,
                        );
                assert(crate::identity_model::budget_ids_equal(
                    ledger.accounts[parent_index as int].id,
                    ledger.accounts[linked].id,
                ));
                crate::invariant::matching_accounts_are_unique(
                    ledger,
                    parent_index as int,
                    linked,
                );
            }
            establish_fault_lineage_safe(
                ledger,
                parent_index,
                parent_id,
                fuel - 1,
            );
        }
    }
    assert forall |candidate: int| #![auto]
        crate::reachability::account_at_guard(ledger, current_id, candidate) implies
            candidate == index as int by {
        assert(crate::identity_model::budget_ids_equal(
            ledger.accounts[candidate].id,
            ledger.accounts[index as int].id,
        ));
        crate::invariant::matching_accounts_are_unique(ledger, candidate, index as int);
    }
    assert(crate::reachability::account_at_guard(
        ledger,
        current_id,
        index as int,
    ));
}

} // verus!
