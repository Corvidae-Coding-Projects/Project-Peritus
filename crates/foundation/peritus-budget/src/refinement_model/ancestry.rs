//! Derive propagation along every finite parent path from the checked immediate-parent relation.

#[cfg(verus_only)]
use crate::BudgetLedger;
use vstd::prelude::*;

verus! {

/// A nonempty sequence of actual account indices following the stored parent identities.
pub(crate) open spec fn ancestor_path(ledger: &BudgetLedger, path: Seq<int>) -> bool {
    path.len() > 0
        && (forall |position: int|
            0 <= position < path.len() ==> 0 <= #[trigger] path[position] < ledger.accounts@.len())
        && (forall |position: int| #![auto]
            0 <= position && position + 1 < path.len() ==>
                crate::identity_model::parent_matches(
                    ledger.accounts[path[position]].parent_id,
                    ledger.accounts[path[position + 1]].id,
                ))
}

/// Every changed descendant has the same consumption delta at every account on its parent path.
pub(crate) open spec fn all_ancestor_deltas(before: &BudgetLedger, after: &BudgetLedger) -> bool {
    before.accounts@.len() <= after.accounts@.len()
        && (forall |path: Seq<int>, position: int| #![auto]
            ancestor_path(before, path)
                && 0 <= position < path.len()
                && !before.accounts[path[0]].consumed.spec_equal(after.accounts[path[0]].consumed)
                ==> super::consumption_delta_equal(
                    before.accounts[path[0]],
                    after.accounts[path[0]],
                    before.accounts[path[position]],
                    after.accounts[path[position]],
                ))
}

pub(crate) proof fn all_ancestor_deltas_follow(before: &BudgetLedger, after: &BudgetLedger)
    requires
        crate::model::ledger_well_formed(before),
        before.accounts@.len() <= after.accounts@.len(),
        super::ancestor_consumption_propagates(before, after),
    ensures all_ancestor_deltas(before, after),
{
    assert forall |path: Seq<int>, position: int| #![auto]
        ancestor_path(before, path)
            && 0 <= position < path.len()
            && !before.accounts[path[0]].consumed.spec_equal(after.accounts[path[0]].consumed)
            implies super::consumption_delta_equal(
                before.accounts[path[0]],
                after.accounts[path[0]],
                before.accounts[path[position]],
                after.accounts[path[position]],
            ) by {
        propagation_to_position(before, after, path, position);
    }
}

proof fn propagation_to_position(
    before: &BudgetLedger,
    after: &BudgetLedger,
    path: Seq<int>,
    position: int,
)
    requires
        crate::model::ledger_well_formed(before),
        before.accounts@.len() <= after.accounts@.len(),
        super::ancestor_consumption_propagates(before, after),
        ancestor_path(before, path),
        0 <= position < path.len(),
        !before.accounts[path[0]].consumed.spec_equal(after.accounts[path[0]].consumed),
    ensures
        super::consumption_delta_equal(
            before.accounts[path[0]],
            after.accounts[path[0]],
            before.accounts[path[position]],
            after.accounts[path[position]],
        ),
        !before.accounts[path[position]].consumed.spec_equal(
            after.accounts[path[position]].consumed,
        ),
    decreases position,
{
    if position > 0 {
        propagation_to_position(before, after, path, position - 1);
        let child = path[position - 1];
        let next = path[position];
        assert(before.accounts[child].parent_id.is_some());
        let parent = choose |parent: int| #![auto]
            0 <= parent < before.accounts@.len()
                && crate::identity_model::parent_matches(
                    before.accounts[child].parent_id,
                    before.accounts[parent].id,
                )
                && super::consumption_delta_equal(
                    before.accounts[child],
                    after.accounts[child],
                    before.accounts[parent],
                    after.accounts[parent],
                );
        assert(crate::identity_model::budget_ids_equal(
            before.accounts[parent].id,
            before.accounts[next].id,
        ));
        crate::invariant::matching_accounts_are_unique(before, parent, next);
        assert(parent == next);
    }
}

} // verus!
