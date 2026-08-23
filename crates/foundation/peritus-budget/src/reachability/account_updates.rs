//! Exact operational account-vector updates for lineage accounting.

#[cfg(verus_only)]
mod preservation;

#[cfg(verus_only)]
pub(crate) use self::preservation::{
    lineage_charge_preserves_account_id, operation_release_preserves_account_identity,
};

#[cfg(verus_only)]
use crate::{BudgetAmounts, BudgetLedger};
#[cfg(verus_only)]
use peritus_types::BudgetId;
use vstd::prelude::*;

verus! {

pub(crate) open spec fn charged_account(
    before: crate::state::BudgetAccount,
    after: crate::state::BudgetAccount,
    amount: BudgetAmounts,
    delegated_child: bool,
) -> bool {
    super::accounts::immutable_account_fields_equal(before, after)
        && BudgetAmounts::spec_sum(after.consumed, before.consumed, amount)
        && before.operation_reserved.spec_equal(after.operation_reserved)
        && if delegated_child {
            BudgetAmounts::spec_sum(
                before.child_delegated_remaining,
                after.child_delegated_remaining,
                amount,
            )
        } else {
            before.child_delegated_remaining.spec_equal(after.child_delegated_remaining)
        }
        && before.phase == after.phase
}

pub(crate) open spec fn lineage_charge_fuel(
    before: Seq<crate::state::BudgetAccount>,
    after: Seq<crate::state::BudgetAccount>,
    current_id: BudgetId,
    amount: BudgetAmounts,
    delegated_child: bool,
    fuel: nat,
) -> bool
    decreases fuel,
{
    fuel > 0
        && exists |witness: (int, crate::state::BudgetAccount)| #![auto]
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
                }
}

pub(crate) open spec fn lineage_charge(
    before: &BudgetLedger,
    after: &BudgetLedger,
    budget_id: BudgetId,
    amount: BudgetAmounts,
) -> bool {
    crate::identity_model::budget_ids_equal(before.root_id, after.root_id)
        && lineage_charge_fuel(
            before.accounts@,
            after.accounts@,
            budget_id,
            amount,
            false,
            before.accounts@.len() as nat,
        )
}

pub(crate) open spec fn faulted_account(
    before: crate::state::BudgetAccount,
    after: crate::state::BudgetAccount,
) -> bool {
    super::accounts::immutable_account_fields_equal(before, after)
        && before.consumed.spec_equal(after.consumed)
        && before.operation_reserved.spec_equal(after.operation_reserved)
        && before.child_delegated_remaining.spec_equal(after.child_delegated_remaining)
        && after.phase == crate::BudgetAccountPhase::Faulted
}

pub(crate) open spec fn lineage_fault_fuel(
    before: Seq<crate::state::BudgetAccount>,
    after: Seq<crate::state::BudgetAccount>,
    current_id: BudgetId,
    fuel: nat,
) -> bool
    decreases fuel,
{
    fuel > 0
        && exists |index: int, updated: crate::state::BudgetAccount| #![auto]
            0 <= index < before.len()
                && crate::identity_model::budget_ids_equal(before[index].id, current_id)
                && faulted_account(before[index], updated)
                && {
                    let intermediate = before.update(index, updated);
                    match before[index].parent_id {
                        None => after == intermediate,
                        Some(parent_id) => lineage_fault_fuel(
                            intermediate,
                            after,
                            parent_id,
                            (fuel - 1) as nat,
                        ),
                    }
                }
}

pub(crate) open spec fn lineage_fault(
    before: &BudgetLedger,
    after: &BudgetLedger,
    budget_id: BudgetId,
) -> bool {
    crate::identity_model::budget_ids_equal(before.root_id, after.root_id)
        && lineage_fault_fuel(
            before.accounts@,
            after.accounts@,
            budget_id,
            before.accounts@.len() as nat,
        )
}

pub(crate) open spec fn overrun_accounting(
    before: &BudgetLedger,
    after: &BudgetLedger,
    released_state: &BudgetLedger,
    charged_state: &BudgetLedger,
    budget_id: BudgetId,
    charged: BudgetAmounts,
) -> bool {
    full_charge_accounting(before, charged_state, released_state, budget_id, charged)
        && lineage_fault(charged_state, after, budget_id)
}

pub(crate) open spec fn released_account(
    before: crate::state::BudgetAccount,
    after: crate::state::BudgetAccount,
    amount: BudgetAmounts,
) -> bool {
    super::accounts::immutable_account_fields_equal(before, after)
        && before.consumed.spec_equal(after.consumed)
        && BudgetAmounts::spec_difference(
            after.operation_reserved,
            before.operation_reserved,
            amount,
        )
        && before.child_delegated_remaining.spec_equal(after.child_delegated_remaining)
        && before.phase == after.phase
}

pub(crate) open spec fn operation_release(
    before: &BudgetLedger,
    after: &BudgetLedger,
    budget_id: BudgetId,
    amount: BudgetAmounts,
) -> bool {
    crate::identity_model::budget_ids_equal(before.root_id, after.root_id)
        && exists |index: int, updated: crate::state::BudgetAccount| #![auto]
            0 <= index < before.accounts@.len()
                && crate::identity_model::budget_ids_equal(before.accounts[index].id, budget_id)
                && released_account(before.accounts[index], updated, amount)
                && after.accounts@ == before.accounts@.update(index, updated)
}

pub(crate) open spec fn reserved_account(
    before: crate::state::BudgetAccount,
    after: crate::state::BudgetAccount,
    amount: BudgetAmounts,
) -> bool {
    super::accounts::immutable_account_fields_equal(before, after)
        && before.consumed.spec_equal(after.consumed)
        && BudgetAmounts::spec_sum(after.operation_reserved, before.operation_reserved, amount)
        && before.child_delegated_remaining.spec_equal(after.child_delegated_remaining)
        && before.phase == after.phase
}

pub(crate) open spec fn operation_reserve(
    before: &BudgetLedger,
    after: &BudgetLedger,
    budget_id: BudgetId,
    amount: BudgetAmounts,
) -> bool {
    crate::identity_model::budget_ids_equal(before.root_id, after.root_id)
        && exists |index: int, updated: crate::state::BudgetAccount| #![auto]
            0 <= index < before.accounts@.len()
                && crate::identity_model::budget_ids_equal(before.accounts[index].id, budget_id)
                && reserved_account(before.accounts[index], updated, amount)
                && after.accounts@ == before.accounts@.update(index, updated)
}

pub(crate) open spec fn begin_accounting(
    before: &BudgetLedger,
    after: &BudgetLedger,
    charged_state: &BudgetLedger,
    budget_id: BudgetId,
    charged: BudgetAmounts,
    reserved: BudgetAmounts,
) -> bool {
    lineage_charge(before, charged_state, budget_id, charged)
        && operation_reserve(charged_state, after, budget_id, reserved)
}

pub(crate) open spec fn full_charge_accounting(
    before: &BudgetLedger,
    after: &BudgetLedger,
    released_state: &BudgetLedger,
    budget_id: BudgetId,
    charged: BudgetAmounts,
) -> bool {
    operation_release(before, released_state, budget_id, charged)
        && lineage_charge(released_state, after, budget_id, charged)
}

pub(crate) open spec fn reservation_accounting(
    before: &BudgetLedger,
    after: &BudgetLedger,
    released_state: &BudgetLedger,
    budget_id: BudgetId,
    charged: BudgetAmounts,
    released: BudgetAmounts,
) -> bool {
    operation_release(before, released_state, budget_id, charged)
        && exists |charged_state: BudgetLedger| #![auto]
            lineage_charge(released_state, &charged_state, budget_id, charged)
                && if released.spec_is_zero() {
                    (crate::identity_model::budget_ids_equal(
                        charged_state.root_id,
                        after.root_id,
                    ) && charged_state.accounts@ == after.accounts@)
                        || operation_release(&charged_state, after, budget_id, released)
                } else {
                    operation_release(&charged_state, after, budget_id, released)
                }
}

} // verus!
