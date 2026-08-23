//! Identity-preservation consequences of lineage accounting updates.

use super::{charged_account, lineage_charge, lineage_charge_fuel, operation_release};
use crate::{BudgetAmounts, BudgetLedger};
use peritus_types::BudgetId;
use vstd::prelude::*;

verus! {

pub(crate) proof fn lineage_charge_preserves_account_id(
    before: &BudgetLedger,
    after: &BudgetLedger,
    budget_id: BudgetId,
    amount: BudgetAmounts,
    query: int,
)
    requires
        lineage_charge(before, after, budget_id, amount),
        0 <= query < before.accounts@.len(),
    ensures
        crate::identity_model::budget_ids_equal(
            before.accounts[query].id,
            after.accounts[query].id,
        ),
        before.accounts[query].operation_reserved.spec_equal(
            after.accounts[query].operation_reserved,
        ),
        crate::identity_model::parents_equal(
            before.accounts[query].parent_id,
            after.accounts[query].parent_id,
        ),
        crate::identity_model::revisions_equal(
            before.accounts[query].revision,
            after.accounts[query].revision,
        ),
        before.accounts[query].limits.spec_amounts().spec_equal(
            after.accounts[query].limits.spec_amounts(),
        ),
{
    lineage_charge_fuel_preserves_account_id(
        before.accounts@,
        after.accounts@,
        budget_id,
        amount,
        false,
        before.accounts@.len() as nat,
        query,
    );
}

proof fn lineage_charge_fuel_preserves_account_id(
    before: Seq<crate::state::BudgetAccount>,
    after: Seq<crate::state::BudgetAccount>,
    current_id: BudgetId,
    amount: BudgetAmounts,
    delegated_child: bool,
    fuel: nat,
    query: int,
)
    requires
        lineage_charge_fuel(
            before,
            after,
            current_id,
            amount,
            delegated_child,
            fuel,
        ),
        0 <= query < before.len(),
    ensures
        crate::identity_model::budget_ids_equal(before[query].id, after[query].id),
        before[query].operation_reserved.spec_equal(after[query].operation_reserved),
        crate::identity_model::parents_equal(
            before[query].parent_id,
            after[query].parent_id,
        ),
        crate::identity_model::revisions_equal(
            before[query].revision,
            after[query].revision,
        ),
        before[query].limits.spec_amounts().spec_equal(after[query].limits.spec_amounts()),
    decreases fuel,
{
    let witness = choose |witness: (int, crate::state::BudgetAccount)| #![auto]
        0 <= witness.0 < before.len()
            && crate::identity_model::budget_ids_equal(before[witness.0].id, current_id)
            && charged_account(before[witness.0], witness.1, amount, delegated_child)
            && {
                let intermediate = before.update(witness.0, witness.1);
                match before[witness.0].parent_id {
                    None => after == intermediate,
                    Some(parent_id) => lineage_charge_fuel(
                        intermediate,
                        after,
                        parent_id,
                        amount,
                        true,
                        (fuel - 1) as nat,
                    ),
                }
            };
    let index = witness.0;
    let updated = witness.1;
    let intermediate = before.update(index, updated);
    assert(crate::identity_model::budget_ids_equal(
        before[query].id,
        intermediate[query].id,
    )) by {
        if query == index {
            assert(super::super::accounts::immutable_account_fields_equal(
                before[index],
                updated,
            ));
        }
    }
    assert(before[query].operation_reserved.spec_equal(
        intermediate[query].operation_reserved,
    )) by {
        if query == index {
            assert(charged_account(before[index], updated, amount, delegated_child));
        }
    }
    assert(crate::identity_model::parents_equal(
        before[query].parent_id,
        intermediate[query].parent_id,
    )) by {
        if query == index {
            assert(charged_account(before[index], updated, amount, delegated_child));
        }
    }
    assert(crate::identity_model::revisions_equal(
        before[query].revision,
        intermediate[query].revision,
    )) by {
        if query == index {
            assert(charged_account(before[index], updated, amount, delegated_child));
        }
    }
    assert(before[query].limits.spec_amounts().spec_equal(
        intermediate[query].limits.spec_amounts(),
    )) by {
        if query == index {
            assert(charged_account(before[index], updated, amount, delegated_child));
        }
    }
    match before[index].parent_id {
        Some(parent_id) => {
            lineage_charge_fuel_preserves_account_id(
                intermediate,
                after,
                parent_id,
                amount,
                true,
                (fuel - 1) as nat,
                query,
            );
        }
        None => {}
    }
}

pub(crate) proof fn operation_release_preserves_account_identity(
    before: &BudgetLedger,
    after: &BudgetLedger,
    budget_id: BudgetId,
    amount: BudgetAmounts,
    query: int,
)
    requires
        operation_release(before, after, budget_id, amount),
        0 <= query < before.accounts@.len(),
    ensures crate::refinement_model::account_identity_stable(before, after, query),
{
}

} // verus!
