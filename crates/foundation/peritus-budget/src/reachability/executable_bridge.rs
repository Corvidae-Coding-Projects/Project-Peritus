//! Specification projections for the executable accounting helpers.

#[cfg(verus_only)]
use crate::BudgetLedger;
use vstd::prelude::*;

verus! {

pub(crate) open spec fn charged_account_exact(
    before: crate::state::BudgetAccount,
    after: crate::state::BudgetAccount,
    amount: crate::BudgetAmounts,
    delegated_child: bool,
) -> bool {
    super::account_updates::charged_account(before, after, amount, delegated_child)
}

pub(crate) open spec fn lineage_charge_fuel_exact(
    before: Seq<crate::state::BudgetAccount>,
    after: Seq<crate::state::BudgetAccount>,
    current_id: peritus_types::BudgetId,
    amount: crate::BudgetAmounts,
    delegated_child: bool,
    fuel: nat,
) -> bool {
    super::account_updates::lineage_charge_fuel(
        before,
        after,
        current_id,
        amount,
        delegated_child,
        fuel,
    )
}

pub(crate) open spec fn lineage_charge_exact(
    before: &BudgetLedger,
    after: &BudgetLedger,
    budget_id: peritus_types::BudgetId,
    amount: crate::BudgetAmounts,
) -> bool {
    super::account_updates::lineage_charge(before, after, budget_id, amount)
}

pub(crate) open spec fn faulted_account_exact(
    before: crate::state::BudgetAccount,
    after: crate::state::BudgetAccount,
) -> bool {
    super::account_updates::faulted_account(before, after)
}

pub(crate) open spec fn lineage_fault_fuel_exact(
    before: Seq<crate::state::BudgetAccount>,
    after: Seq<crate::state::BudgetAccount>,
    current_id: peritus_types::BudgetId,
    fuel: nat,
) -> bool {
    super::account_updates::lineage_fault_fuel(before, after, current_id, fuel)
}

pub(crate) open spec fn lineage_fault_exact(
    before: &BudgetLedger,
    after: &BudgetLedger,
    budget_id: peritus_types::BudgetId,
) -> bool {
    super::account_updates::lineage_fault(before, after, budget_id)
}

pub(crate) open spec fn overrun_accounting_exact(
    before: &BudgetLedger,
    after: &BudgetLedger,
    released_state: &BudgetLedger,
    charged_state: &BudgetLedger,
    budget_id: peritus_types::BudgetId,
    charged: crate::BudgetAmounts,
) -> bool {
    super::account_updates::overrun_accounting(
        before,
        after,
        released_state,
        charged_state,
        budget_id,
        charged,
    )
}

pub(crate) open spec fn observation_accounting_exact(
    before: &BudgetLedger,
    after: &BudgetLedger,
    released_state: &BudgetLedger,
    budget_id: peritus_types::BudgetId,
    charged: crate::BudgetAmounts,
    released: crate::BudgetAmounts,
) -> bool {
    super::account_updates::reservation_accounting(
        before,
        after,
        released_state,
        budget_id,
        charged,
        released,
    )
}

pub(crate) open spec fn released_account_exact(
    before: crate::state::BudgetAccount,
    after: crate::state::BudgetAccount,
    amount: crate::BudgetAmounts,
) -> bool {
    super::account_updates::released_account(before, after, amount)
}

pub(crate) open spec fn operation_release_exact(
    before: &BudgetLedger,
    after: &BudgetLedger,
    budget_id: peritus_types::BudgetId,
    amount: crate::BudgetAmounts,
) -> bool {
    super::account_updates::operation_release(before, after, budget_id, amount)
}

pub(crate) open spec fn reserved_account_exact(
    before: crate::state::BudgetAccount,
    after: crate::state::BudgetAccount,
    amount: crate::BudgetAmounts,
) -> bool {
    super::account_updates::reserved_account(before, after, amount)
}

pub(crate) open spec fn operation_reserve_exact(
    before: &BudgetLedger,
    after: &BudgetLedger,
    budget_id: peritus_types::BudgetId,
    amount: crate::BudgetAmounts,
) -> bool {
    super::account_updates::operation_reserve(before, after, budget_id, amount)
}

pub(crate) open spec fn begin_accounting_exact(
    before: &BudgetLedger,
    after: &BudgetLedger,
    charged_state: &BudgetLedger,
    budget_id: peritus_types::BudgetId,
    charged: crate::BudgetAmounts,
    reserved: crate::BudgetAmounts,
) -> bool {
    super::account_updates::begin_accounting(
        before,
        after,
        charged_state,
        budget_id,
        charged,
        reserved,
    )
}

pub(crate) proof fn lineage_charge_preserves_account_id(
    before: &BudgetLedger,
    after: &BudgetLedger,
    budget_id: peritus_types::BudgetId,
    amount: crate::BudgetAmounts,
    query: int,
)
    requires
        lineage_charge_exact(before, after, budget_id, amount),
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
    super::account_updates::lineage_charge_preserves_account_id(
        before, after, budget_id, amount, query,
    );
}

pub(crate) proof fn operation_release_preserves_account_identity(
    before: &BudgetLedger,
    after: &BudgetLedger,
    budget_id: peritus_types::BudgetId,
    amount: crate::BudgetAmounts,
    query: int,
)
    requires
        operation_release_exact(before, after, budget_id, amount),
        0 <= query < before.accounts@.len(),
    ensures crate::refinement_model::account_identity_stable(before, after, query),
{
    super::account_updates::operation_release_preserves_account_identity(
        before, after, budget_id, amount, query,
    );
}

pub(crate) open spec fn reservation_accounting_exact(
    before: &BudgetLedger,
    after: &BudgetLedger,
    released_state: &BudgetLedger,
    budget_id: peritus_types::BudgetId,
    charged: crate::BudgetAmounts,
) -> bool {
    super::account_updates::full_charge_accounting(
        before,
        after,
        released_state,
        budget_id,
        charged,
    )
}

} // verus!
