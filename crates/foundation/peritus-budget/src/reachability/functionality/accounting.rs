//! Functionality of recursive lineage and single-account accounting effects.

#[cfg(verus_only)]
use crate::{BudgetAmounts, BudgetLedger};
#[cfg(verus_only)]
use peritus_types::BudgetId;
use vstd::prelude::*;

verus! {

pub(super) open spec fn account_sequences_equal(
    left: Seq<crate::state::BudgetAccount>,
    right: Seq<crate::state::BudgetAccount>,
) -> bool {
    left.len() == right.len()
        && forall |index: int| #![auto]
            0 <= index < left.len()
                ==> super::super::accounts::account_exactly_equal(
                    left[index], right[index],
                )
}

pub(super) open spec fn account_ids_unique(accounts: Seq<crate::state::BudgetAccount>) -> bool {
    forall |left: int, right: int| #![auto]
        0 <= left < right < accounts.len()
            ==> !crate::identity_model::budget_ids_equal(
                accounts[left].id, accounts[right].id,
            )
}

pub(super) proof fn well_formed_has_unique_account_ids(ledger: &BudgetLedger)
    requires crate::model::ledger_well_formed(ledger),
    ensures account_ids_unique(ledger.accounts@),
{
    assert forall |left: int, right: int| #![auto]
        0 <= left < right < ledger.accounts@.len()
            implies !crate::identity_model::budget_ids_equal(
                ledger.accounts[left].id, ledger.accounts[right].id,
            ) by {
        assert(crate::invariant::account_entry_valid(ledger, right));
        assert(crate::invariant::account_unique_before(ledger, right));
    }
}

pub(super) proof fn updated_ids_remain_unique(
    before: Seq<crate::state::BudgetAccount>,
    updated: crate::state::BudgetAccount,
    index: int,
)
    requires
        account_ids_unique(before),
        0 <= index < before.len(),
        crate::identity_model::budget_ids_equal(before[index].id, updated.id),
    ensures account_ids_unique(before.update(index, updated)),
{
    assert forall |left: int, right: int| #![auto]
        0 <= left < right < before.len()
            implies !crate::identity_model::budget_ids_equal(
                before.update(index, updated)[left].id,
                before.update(index, updated)[right].id,
            ) by {
        if left == index {
            assert(!crate::identity_model::budget_ids_equal(
                before[index].id, before[right].id,
            ));
            if crate::identity_model::budget_ids_equal(
                updated.id, before[right].id,
            ) {
                crate::identity_model::budget_ids_transitive(
                    before[index].id, updated.id, before[right].id,
                );
                assert(false);
            }
        } else if right == index {
            assert(!crate::identity_model::budget_ids_equal(
                before[left].id, before[index].id,
            ));
            if crate::identity_model::budget_ids_equal(
                before[left].id, updated.id,
            ) {
                crate::identity_model::budget_ids_symmetric(
                    before[index].id, updated.id,
                );
                crate::identity_model::budget_ids_transitive(
                    before[left].id, updated.id, before[index].id,
                );
                assert(false);
            }
        }
    }
}

proof fn charged_updates_match(
    left_before: Seq<crate::state::BudgetAccount>,
    right_before: Seq<crate::state::BudgetAccount>,
    left_updated: crate::state::BudgetAccount,
    right_updated: crate::state::BudgetAccount,
    index: int,
    left_amount: BudgetAmounts,
    right_amount: BudgetAmounts,
    delegated_child: bool,
)
    requires
        account_sequences_equal(left_before, right_before),
        left_amount.spec_equal(right_amount),
        0 <= index < left_before.len(),
        super::super::account_updates::charged_account(
            left_before[index], left_updated, left_amount, delegated_child,
        ),
        super::super::account_updates::charged_account(
            right_before[index], right_updated, right_amount, delegated_child,
        ),
    ensures
        super::super::accounts::account_exactly_equal(left_updated, right_updated),
        account_sequences_equal(
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

pub(super) proof fn lineage_charge_fuel_functional(
    left_before: Seq<crate::state::BudgetAccount>,
    right_before: Seq<crate::state::BudgetAccount>,
    left_after: Seq<crate::state::BudgetAccount>,
    right_after: Seq<crate::state::BudgetAccount>,
    left_current: BudgetId,
    right_current: BudgetId,
    left_amount: BudgetAmounts,
    right_amount: BudgetAmounts,
    delegated_child: bool,
    fuel: nat,
)
    requires
        account_sequences_equal(left_before, right_before),
        account_ids_unique(left_before),
        crate::identity_model::budget_ids_equal(left_current, right_current),
        left_amount.spec_equal(right_amount),
        super::super::account_updates::lineage_charge_fuel(
            left_before,
            left_after,
            left_current,
            left_amount,
            delegated_child,
            fuel,
        ),
        super::super::account_updates::lineage_charge_fuel(
            right_before,
            right_after,
            right_current,
            right_amount,
            delegated_child,
            fuel,
        ),
    ensures
        account_sequences_equal(left_after, right_after),
        account_ids_unique(left_after),
    decreases fuel,
{
    let left_witness = choose |witness: (int, crate::state::BudgetAccount)| #![auto]
        0 <= witness.0 < left_before.len()
            && crate::identity_model::budget_ids_equal(
                left_before[witness.0].id, left_current,
            )
            && super::super::account_updates::charged_account(
                left_before[witness.0], witness.1, left_amount, delegated_child,
            )
            && {
                let intermediate = left_before.update(witness.0, witness.1);
                match left_before[witness.0].parent_id {
                    None => left_after == intermediate,
                    Some(parent_id) => super::super::account_updates::lineage_charge_fuel(
                        intermediate,
                        left_after,
                        parent_id,
                        left_amount,
                        true,
                        (fuel - 1) as nat,
                    ),
                }
            };
    let right_witness = choose |witness: (int, crate::state::BudgetAccount)| #![auto]
        0 <= witness.0 < right_before.len()
            && crate::identity_model::budget_ids_equal(
                right_before[witness.0].id, right_current,
            )
            && super::super::account_updates::charged_account(
                right_before[witness.0], witness.1, right_amount, delegated_child,
            )
            && {
                let intermediate = right_before.update(witness.0, witness.1);
                match right_before[witness.0].parent_id {
                    None => right_after == intermediate,
                    Some(parent_id) => super::super::account_updates::lineage_charge_fuel(
                        intermediate,
                        right_after,
                        parent_id,
                        right_amount,
                        true,
                        (fuel - 1) as nat,
                    ),
                }
            };
    let left_index = left_witness.0;
    let right_index = right_witness.0;
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
    let left_intermediate = left_before.update(left_index, left_witness.1);
    let right_intermediate = right_before.update(right_index, right_witness.1);
    charged_updates_match(
        left_before,
        right_before,
        left_witness.1,
        right_witness.1,
        left_index,
        left_amount,
        right_amount,
        delegated_child,
    );
    updated_ids_remain_unique(left_before, left_witness.1, left_index);
    match (left_before[left_index].parent_id, right_before[right_index].parent_id) {
        (None, None) => {}
        (Some(left_parent), Some(right_parent)) => {
            lineage_charge_fuel_functional(
                left_intermediate,
                right_intermediate,
                left_after,
                right_after,
                left_parent,
                right_parent,
                left_amount,
                right_amount,
                true,
                (fuel - 1) as nat,
            );
        }
        _ => {
            assert(super::super::accounts::account_exactly_equal(
                left_before[left_index], right_before[right_index],
            ));
            assert(crate::identity_model::parents_equal(
                left_before[left_index].parent_id,
                right_before[right_index].parent_id,
            ));
            reveal(crate::identity_model::parents_equal);
            assert(false);
        }
    }
}

proof fn reserved_updates_match(
    left_before: Seq<crate::state::BudgetAccount>,
    right_before: Seq<crate::state::BudgetAccount>,
    left_updated: crate::state::BudgetAccount,
    right_updated: crate::state::BudgetAccount,
    index: int,
    amount: BudgetAmounts,
)
    requires
        account_sequences_equal(left_before, right_before),
        0 <= index < left_before.len(),
        super::super::account_updates::reserved_account(
            left_before[index], left_updated, amount,
        ),
        super::super::account_updates::reserved_account(
            right_before[index], right_updated, amount,
        ),
    ensures
        super::super::accounts::account_exactly_equal(left_updated, right_updated),
        account_sequences_equal(
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

pub(super) proof fn operation_reserve_functional(
    left_before: &BudgetLedger,
    right_before: &BudgetLedger,
    left_after: &BudgetLedger,
    right_after: &BudgetLedger,
    left_budget: BudgetId,
    right_budget: BudgetId,
    amount: BudgetAmounts,
)
    requires
        account_sequences_equal(left_before.accounts@, right_before.accounts@),
        account_ids_unique(left_before.accounts@),
        crate::identity_model::budget_ids_equal(left_budget, right_budget),
        super::super::account_updates::operation_reserve(
            left_before, left_after, left_budget, amount,
        ),
        super::super::account_updates::operation_reserve(
            right_before, right_after, right_budget, amount,
        ),
    ensures
        account_sequences_equal(left_after.accounts@, right_after.accounts@),
        account_ids_unique(left_after.accounts@),
{
    reveal(super::super::account_updates::operation_reserve);
    let left_index = choose |index: int| #![auto]
        0 <= index < left_before.accounts@.len()
            && crate::identity_model::budget_ids_equal(
                left_before.accounts[index].id, left_budget,
            )
            && exists |updated: crate::state::BudgetAccount| #![auto]
            super::super::account_updates::reserved_account(
                left_before.accounts[index], updated, amount,
            )
            && left_after.accounts@
                == left_before.accounts@.update(index, updated);
    let left_updated = choose |updated: crate::state::BudgetAccount| #![auto]
            super::super::account_updates::reserved_account(
                left_before.accounts[left_index], updated, amount,
            )
            && left_after.accounts@
                == left_before.accounts@.update(left_index, updated);
    let right_index = choose |index: int| #![auto]
        0 <= index < right_before.accounts@.len()
            && crate::identity_model::budget_ids_equal(
                right_before.accounts[index].id, right_budget,
            )
            && exists |updated: crate::state::BudgetAccount| #![auto]
            super::super::account_updates::reserved_account(
                right_before.accounts[index], updated, amount,
            )
            && right_after.accounts@
                == right_before.accounts@.update(index, updated);
    let right_updated = choose |updated: crate::state::BudgetAccount| #![auto]
            super::super::account_updates::reserved_account(
                right_before.accounts[right_index], updated, amount,
            )
            && right_after.accounts@
                == right_before.accounts@.update(right_index, updated);
    assert(crate::identity_model::budget_ids_equal(
        left_before.accounts[left_index].id, left_before.accounts[right_index].id,
    ));
    if left_index < right_index {
        assert(false);
    } else if right_index < left_index {
        crate::identity_model::budget_ids_symmetric(
            left_before.accounts[left_index].id,
            left_before.accounts[right_index].id,
        );
        assert(false);
    }
    assert(left_index == right_index);
    reserved_updates_match(
        left_before.accounts@,
        right_before.accounts@,
        left_updated,
        right_updated,
        left_index,
        amount,
    );
    updated_ids_remain_unique(left_before.accounts@, left_updated, left_index);
}

} // verus!
