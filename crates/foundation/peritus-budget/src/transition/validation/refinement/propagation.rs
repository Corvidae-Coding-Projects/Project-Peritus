//! Ancestor consumption-propagation validation.

use super::account_at;
use crate::{BudgetError, BudgetLedger};
use vstd::prelude::*;

verus! {

pub(super) fn validate_ancestor_propagation(
    before: &BudgetLedger,
    after: &BudgetLedger,
) -> (result: Result<(), BudgetError>)
    ensures
        (crate::model::ledger_well_formed(before)
            && crate::model::ledger_consumption_monotonic(before, after)
            && crate::refinement_model::ledger_identity_stable(before, after)
            && crate::refinement_model::ancestor_consumption_propagates(before, after))
            ==> result.is_ok(),
        match result {
            Ok(()) => crate::refinement_model::ancestor_consumption_propagates(before, after),
            Err(_) => true,
        },
{
    if before.accounts.len() > after.accounts.len() {
        return Err(crate::model::corrupt(after.root_id));
    }
    let mut child_index = 0;
    while child_index < before.accounts.len()
        invariant
            0 <= child_index <= before.accounts.len(),
            before.accounts@.len() <= after.accounts@.len(),
            (crate::model::ledger_well_formed(before)
                && crate::model::ledger_consumption_monotonic(before, after)
                && crate::refinement_model::ledger_identity_stable(before, after)
                && crate::refinement_model::ancestor_consumption_propagates(before, after))
                ==> before.accounts@.len() <= after.accounts@.len(),
            forall |checked: int| #![auto]
                0 <= checked < child_index
                    && before.accounts[checked].parent_id.is_some()
                    && !before.accounts[checked].consumed.spec_equal(
                        after.accounts[checked].consumed,
                    )
                    ==> exists |parent: int| #![auto]
                        0 <= parent < before.accounts@.len()
                            && crate::identity_model::parent_matches(
                                before.accounts[checked].parent_id,
                                before.accounts[parent].id,
                            )
                            && crate::refinement_model::consumption_delta_equal(
                                before.accounts[checked],
                                after.accounts[checked],
                                before.accounts[parent],
                                after.accounts[parent],
                            ),
        decreases before.accounts.len() - child_index,
    {
        validate_child_propagation(before, after, child_index)?;
        child_index += 1;
    }
    assert(crate::refinement_model::ancestor_consumption_propagates(before, after));
    Ok(())
}

fn validate_child_propagation(
    before: &BudgetLedger,
    after: &BudgetLedger,
    child_index: usize,
) -> (result: Result<(), BudgetError>)
    requires
        child_index < before.accounts@.len(),
        before.accounts@.len() <= after.accounts@.len(),
    ensures
        (crate::model::ledger_well_formed(before)
            && crate::model::ledger_consumption_monotonic(before, after)
            && crate::refinement_model::ledger_identity_stable(before, after)
            && crate::refinement_model::ancestor_consumption_propagates(before, after))
            ==> result.is_ok(),
        match result {
            Ok(()) => {
                before.accounts[child_index as int].parent_id.is_some()
                    && !before.accounts[child_index as int].consumed.spec_equal(
                        after.accounts[child_index as int].consumed,
                    )
                    ==> exists |parent: int| #![auto]
                        0 <= parent < before.accounts@.len()
                            && crate::identity_model::parent_matches(
                                before.accounts[child_index as int].parent_id,
                                before.accounts[parent].id,
                            )
                            && crate::refinement_model::consumption_delta_equal(
                                before.accounts[child_index as int],
                                after.accounts[child_index as int],
                                before.accounts[parent],
                                after.accounts[parent],
                            )
            }
            Err(_) => true,
        },
{
    let prior_child = account_at(before, child_index)?;
    let next_child = account_at(after, child_index)?;
    let changed = !prior_child.consumed.equals(next_child.consumed);
    if prior_child.parent_id.is_some() && changed {
        let parent_index = find_parent_index(before, child_index)?;
        let prior_parent = account_at(before, parent_index)?;
        let next_parent = account_at(after, parent_index)?;
        proof {
            if crate::model::ledger_well_formed(before)
                && crate::model::ledger_consumption_monotonic(before, after)
                && crate::refinement_model::ledger_identity_stable(before, after)
                && crate::refinement_model::ancestor_consumption_propagates(before, after)
            {
                let expected = choose |expected: int| #![auto]
                    0 <= expected < before.accounts@.len()
                        && crate::identity_model::parent_matches(
                            before.accounts[child_index as int].parent_id,
                            before.accounts[expected].id,
                        )
                        && crate::refinement_model::consumption_delta_equal(
                            before.accounts[child_index as int],
                            after.accounts[child_index as int],
                            before.accounts[expected],
                            after.accounts[expected],
                        );
                assert(crate::identity_model::budget_ids_equal(
                    before.accounts[parent_index as int].id,
                    before.accounts[expected].id,
                ));
                crate::invariant::matching_accounts_are_unique(
                    before,
                    parent_index as int,
                    expected,
                );
                assert(parent_index as int == expected);
                assert(crate::model::consumption_monotonic(
                    before.accounts[child_index as int],
                    after.accounts[child_index as int],
                ));
                assert(crate::model::consumption_monotonic(
                    before.accounts[parent_index as int],
                    after.accounts[parent_index as int],
                ));
            }
        }
        let child_delta = next_child
            .consumed
            .checked_sub(prior_child.consumed)
            .map_err(BudgetError::arithmetic)?;
        let parent_delta = next_parent
            .consumed
            .checked_sub(prior_parent.consumed)
            .map_err(BudgetError::arithmetic)?;
        if !child_delta.equals(parent_delta) {
            proof {
                if crate::model::ledger_well_formed(before)
                    && crate::model::ledger_consumption_monotonic(before, after)
                    && crate::refinement_model::ledger_identity_stable(before, after)
                    && crate::refinement_model::ancestor_consumption_propagates(before, after)
                {
                    assert(crate::refinement_model::consumption_delta_equal(
                        before.accounts[child_index as int],
                        after.accounts[child_index as int],
                        before.accounts[parent_index as int],
                        after.accounts[parent_index as int],
                    ));
                    assert(child_delta.spec_equal(parent_delta));
                }
            }
            return Err(crate::model::corrupt(prior_child.id));
        }
        assert(crate::refinement_model::consumption_delta_equal(
            before.accounts[child_index as int],
            after.accounts[child_index as int],
            before.accounts[parent_index as int],
            after.accounts[parent_index as int],
        ));
        assert(exists |parent: int| #![auto]
            0 <= parent < before.accounts@.len()
                && crate::identity_model::parent_matches(
                    before.accounts[child_index as int].parent_id,
                    before.accounts[parent].id,
                )
                && crate::refinement_model::consumption_delta_equal(
                    before.accounts[child_index as int],
                    after.accounts[child_index as int],
                    before.accounts[parent],
                    after.accounts[parent],
                ));
    }
    Ok(())
}

fn find_parent_index(
    ledger: &BudgetLedger,
    child_index: usize,
) -> (result: Result<usize, BudgetError>)
    ensures
        (crate::invariant::ledger_account_structure_holds(ledger)
            && child_index < ledger.accounts@.len()
            && ledger.accounts[child_index as int].parent_id.is_some())
            ==> result.is_ok(),
        match result {
            Ok(parent) => {
                (child_index as int) < ledger.accounts@.len()
                    && (parent as int) < ledger.accounts@.len()
                    && crate::identity_model::parent_matches(
                        ledger.accounts[child_index as int].parent_id,
                        ledger.accounts[parent as int].id,
                    )
            }
            Err(_) => true,
        },
{
    if child_index >= ledger.accounts.len() {
        return Err(crate::model::corrupt(ledger.root_id));
    }
    if child_index > 0 {
        super::super::structure::validate_parent_before(ledger, child_index)?;
    }
    let mut index = 0;
    while index < ledger.accounts.len()
        invariant
            0 <= index <= ledger.accounts.len(),
            (child_index as int) < ledger.accounts@.len(),
            crate::invariant::ledger_account_structure_holds(ledger)
                && ledger.accounts[child_index as int].parent_id.is_some()
                ==> crate::invariant::parent_link_valid(ledger, child_index as int),
            forall |prior: int| #![auto]
                0 <= prior < index
                    ==> !crate::identity_model::parent_matches(
                        ledger.accounts[child_index as int].parent_id,
                        ledger.accounts[prior].id,
                    ),
        decreases ledger.accounts.len() - index,
    {
        if crate::identity_model::parent_matches_id(
            ledger.accounts[child_index].parent_id,
            ledger.accounts[index].id,
        ) {
            return Ok(index);
        }
        assert(!crate::identity_model::parent_matches(
            ledger.accounts[child_index as int].parent_id,
            ledger.accounts[index as int].id,
        ));
        index += 1;
    }
    proof {
        if crate::invariant::ledger_account_structure_holds(ledger)
            && ledger.accounts[child_index as int].parent_id.is_some()
        {
            assert(crate::invariant::parent_link_valid(ledger, child_index as int));
            let expected = choose |expected: int| #![auto]
                0 <= expected < child_index
                    && crate::identity_model::parent_matches(
                        ledger.accounts[child_index as int].parent_id,
                        ledger.accounts[expected].id,
                    )
                    && crate::identity_model::revisions_equal(
                        ledger.accounts[child_index as int].revision,
                        ledger.accounts[expected].revision,
                    );
            assert(expected < index);
            assert(false);
        }
    }
    Err(crate::model::corrupt(ledger.accounts[child_index].id))
}


} // verus!
