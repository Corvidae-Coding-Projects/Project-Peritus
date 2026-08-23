//! Functionality of reservation-release accounting effects.

#[cfg(verus_only)]
use crate::{BudgetAmounts, BudgetLedger};
#[cfg(verus_only)]
use peritus_types::BudgetId;
use vstd::prelude::*;

verus! {

proof fn released_updates_match(
    left_before: Seq<crate::state::BudgetAccount>,
    right_before: Seq<crate::state::BudgetAccount>,
    left_updated: crate::state::BudgetAccount,
    right_updated: crate::state::BudgetAccount,
    index: int,
    left_amount: BudgetAmounts,
    right_amount: BudgetAmounts,
)
    requires
        super::accounting::account_sequences_equal(left_before, right_before),
        left_amount.spec_equal(right_amount),
        0 <= index < left_before.len(),
        super::super::account_updates::released_account(
            left_before[index], left_updated, left_amount,
        ),
        super::super::account_updates::released_account(
            right_before[index], right_updated, right_amount,
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

pub(super) proof fn operation_release_functional(
    left_before: &BudgetLedger,
    right_before: &BudgetLedger,
    left_after: &BudgetLedger,
    right_after: &BudgetLedger,
    left_budget: BudgetId,
    right_budget: BudgetId,
    left_amount: BudgetAmounts,
    right_amount: BudgetAmounts,
)
    requires
        super::accounting::account_sequences_equal(
            left_before.accounts@, right_before.accounts@,
        ),
        super::accounting::account_ids_unique(left_before.accounts@),
        crate::identity_model::budget_ids_equal(left_budget, right_budget),
        left_amount.spec_equal(right_amount),
        super::super::account_updates::operation_release(
            left_before, left_after, left_budget, left_amount,
        ),
        super::super::account_updates::operation_release(
            right_before, right_after, right_budget, right_amount,
        ),
    ensures
        super::accounting::account_sequences_equal(
            left_after.accounts@, right_after.accounts@,
        ),
        super::accounting::account_ids_unique(left_after.accounts@),
{
    reveal(super::super::account_updates::operation_release);
    let left_index = choose |index: int| #![auto]
        0 <= index < left_before.accounts@.len()
            && crate::identity_model::budget_ids_equal(
                left_before.accounts[index].id, left_budget,
            )
            && exists |updated: crate::state::BudgetAccount| #![auto]
                super::super::account_updates::released_account(
                    left_before.accounts[index], updated, left_amount,
                )
                && left_after.accounts@
                    == left_before.accounts@.update(index, updated);
    let left_updated = choose |updated: crate::state::BudgetAccount| #![auto]
        super::super::account_updates::released_account(
            left_before.accounts[left_index], updated, left_amount,
        )
            && left_after.accounts@
                == left_before.accounts@.update(left_index, updated);
    let right_index = choose |index: int| #![auto]
        0 <= index < right_before.accounts@.len()
            && crate::identity_model::budget_ids_equal(
                right_before.accounts[index].id, right_budget,
            )
            && exists |updated: crate::state::BudgetAccount| #![auto]
                super::super::account_updates::released_account(
                    right_before.accounts[index], updated, right_amount,
                )
                && right_after.accounts@
                    == right_before.accounts@.update(index, updated);
    let right_updated = choose |updated: crate::state::BudgetAccount| #![auto]
        super::super::account_updates::released_account(
            right_before.accounts[right_index], updated, right_amount,
        )
            && right_after.accounts@
                == right_before.accounts@.update(right_index, updated);
    assert(crate::identity_model::budget_ids_equal(
        left_before.accounts[left_index].id,
        left_before.accounts[right_index].id,
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
    released_updates_match(
        left_before.accounts@,
        right_before.accounts@,
        left_updated,
        right_updated,
        left_index,
        left_amount,
        right_amount,
    );
    super::accounting::updated_ids_remain_unique(
        left_before.accounts@, left_updated, left_index,
    );
}

proof fn zero_release_preserves_accounts(
    before: &BudgetLedger,
    after: &BudgetLedger,
    budget_id: BudgetId,
    zero: BudgetAmounts,
)
    requires
        zero.spec_is_zero(),
        super::super::account_updates::operation_release(
            before, after, budget_id, zero,
        ),
    ensures super::accounting::account_sequences_equal(
        before.accounts@, after.accounts@,
    ),
{
    reveal(super::super::account_updates::operation_release);
    let index = choose |index: int| #![auto]
        0 <= index < before.accounts@.len()
            && crate::identity_model::budget_ids_equal(
                before.accounts[index].id, budget_id,
            )
            && exists |updated: crate::state::BudgetAccount| #![auto]
                super::super::account_updates::released_account(
                    before.accounts[index], updated, zero,
                )
                && after.accounts@ == before.accounts@.update(index, updated);
    assert forall |query: int| #![auto]
        0 <= query < before.accounts@.len()
            implies super::super::accounts::account_exactly_equal(
                before.accounts[query], after.accounts[query],
            ) by {
    }
}

pub(super) proof fn full_charge_functional(
    left_before: &BudgetLedger,
    right_before: &BudgetLedger,
    left_after: &BudgetLedger,
    right_after: &BudgetLedger,
    left_budget: BudgetId,
    right_budget: BudgetId,
    left_charged: BudgetAmounts,
    right_charged: BudgetAmounts,
)
    requires
        super::accounting::account_sequences_equal(
            left_before.accounts@, right_before.accounts@,
        ),
        super::accounting::account_ids_unique(left_before.accounts@),
        crate::identity_model::budget_ids_equal(left_budget, right_budget),
        left_charged.spec_equal(right_charged),
        exists |released: BudgetLedger| #![auto]
            super::super::account_updates::full_charge_accounting(
                left_before, left_after, &released, left_budget, left_charged,
            ),
        exists |released: BudgetLedger| #![auto]
            super::super::account_updates::full_charge_accounting(
                right_before, right_after, &released, right_budget, right_charged,
            ),
    ensures
        super::accounting::account_sequences_equal(
            left_after.accounts@, right_after.accounts@,
        ),
        super::accounting::account_ids_unique(left_after.accounts@),
{
    reveal(super::super::account_updates::full_charge_accounting);
    let left_released = choose |released: BudgetLedger| #![auto]
        super::super::account_updates::full_charge_accounting(
            left_before, left_after, &released, left_budget, left_charged,
        );
    let right_released = choose |released: BudgetLedger| #![auto]
        super::super::account_updates::full_charge_accounting(
            right_before, right_after, &released, right_budget, right_charged,
        );
    operation_release_functional(
        left_before, right_before, &left_released, &right_released,
        left_budget, right_budget, left_charged, right_charged,
    );
    super::accounting::lineage_charge_fuel_functional(
        left_released.accounts@,
        right_released.accounts@,
        left_after.accounts@,
        right_after.accounts@,
        left_budget,
        right_budget,
        left_charged,
        right_charged,
        false,
        left_released.accounts@.len() as nat,
    );
}

pub(super) proof fn reservation_accounting_functional(
    left_before: &BudgetLedger,
    right_before: &BudgetLedger,
    left_after: &BudgetLedger,
    right_after: &BudgetLedger,
    left_budget: BudgetId,
    right_budget: BudgetId,
    left_charged_amount: BudgetAmounts,
    right_charged_amount: BudgetAmounts,
    left_released_amount: BudgetAmounts,
    right_released_amount: BudgetAmounts,
)
    requires
        super::accounting::account_sequences_equal(
            left_before.accounts@, right_before.accounts@,
        ),
        super::accounting::account_ids_unique(left_before.accounts@),
        crate::identity_model::budget_ids_equal(left_budget, right_budget),
        left_charged_amount.spec_equal(right_charged_amount),
        left_released_amount.spec_equal(right_released_amount),
        exists |released_state: BudgetLedger| #![auto]
            super::super::account_updates::reservation_accounting(
                left_before, left_after, &released_state,
                left_budget, left_charged_amount, left_released_amount,
            ),
        exists |released_state: BudgetLedger| #![auto]
            super::super::account_updates::reservation_accounting(
                right_before, right_after, &released_state,
                right_budget, right_charged_amount, right_released_amount,
            ),
    ensures super::accounting::account_sequences_equal(
        left_after.accounts@, right_after.accounts@,
    ),
{
    reveal(super::super::account_updates::reservation_accounting);
    let left_released = choose |state: BudgetLedger| #![auto]
        super::super::account_updates::reservation_accounting(
            left_before, left_after, &state,
            left_budget, left_charged_amount, left_released_amount,
        );
    let right_released = choose |state: BudgetLedger| #![auto]
        super::super::account_updates::reservation_accounting(
            right_before, right_after, &state,
            right_budget, right_charged_amount, right_released_amount,
        );
    operation_release_functional(
        left_before, right_before, &left_released, &right_released,
        left_budget, right_budget, left_charged_amount, right_charged_amount,
    );
    let left_charged = choose |state: BudgetLedger| #![auto]
        super::super::account_updates::lineage_charge(
            &left_released, &state, left_budget, left_charged_amount,
        ) && if left_released_amount.spec_is_zero() {
            (crate::identity_model::budget_ids_equal(state.root_id, left_after.root_id)
                && state.accounts@ == left_after.accounts@)
                || super::super::account_updates::operation_release(
                    &state, left_after, left_budget, left_released_amount,
                )
        } else {
            super::super::account_updates::operation_release(
                &state, left_after, left_budget, left_released_amount,
            )
        };
    let right_charged = choose |state: BudgetLedger| #![auto]
        super::super::account_updates::lineage_charge(
            &right_released, &state, right_budget, right_charged_amount,
        ) && if right_released_amount.spec_is_zero() {
            (crate::identity_model::budget_ids_equal(state.root_id, right_after.root_id)
                && state.accounts@ == right_after.accounts@)
                || super::super::account_updates::operation_release(
                    &state, right_after, right_budget, right_released_amount,
                )
        } else {
            super::super::account_updates::operation_release(
                &state, right_after, right_budget, right_released_amount,
            )
        };
    reveal(super::super::account_updates::lineage_charge);
    super::accounting::lineage_charge_fuel_functional(
        left_released.accounts@,
        right_released.accounts@,
        left_charged.accounts@,
        right_charged.accounts@,
        left_budget,
        right_budget,
        left_charged_amount,
        right_charged_amount,
        false,
        left_released.accounts@.len() as nat,
    );
    if left_released_amount.spec_is_zero() {
        if super::super::account_updates::operation_release(
            &left_charged, left_after, left_budget, left_released_amount,
        ) {
            zero_release_preserves_accounts(
                &left_charged, left_after, left_budget, left_released_amount,
            );
        }
        if super::super::account_updates::operation_release(
            &right_charged, right_after, right_budget, right_released_amount,
        ) {
            zero_release_preserves_accounts(
                &right_charged, right_after, right_budget, right_released_amount,
            );
        }
    } else {
        operation_release_functional(
            &left_charged, &right_charged, left_after, right_after,
            left_budget, right_budget,
            left_released_amount, right_released_amount,
        );
    }
}

} // verus!
