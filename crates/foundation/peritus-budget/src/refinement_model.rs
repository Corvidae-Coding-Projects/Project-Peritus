//! Concrete immutable-prefix and ancestor-propagation refinement predicates.

#[cfg(verus_only)]
use crate::{BudgetDimension, BudgetLedger};
use vstd::prelude::*;

verus! {

pub(crate) open spec fn ledger_identity_stable(
    before: &BudgetLedger,
    after: &BudgetLedger,
) -> bool {
    crate::identity_model::budget_ids_equal(before.root_id, after.root_id)
        && before.accounts@.len() <= after.accounts@.len()
        && (forall |index: int| #![auto]
            0 <= index < before.accounts@.len()
                ==> account_identity_stable(before, after, index))
        && before.reservations@.len() <= after.reservations@.len()
        && (forall |index: int| #![auto]
            0 <= index < before.reservations@.len()
                ==> reservation_identity_stable(before, after, index))
}

pub(crate) open spec fn account_identity_stable(
    before: &BudgetLedger,
    after: &BudgetLedger,
    index: int,
) -> bool {
    crate::identity_model::budget_ids_equal(
        before.accounts[index].id,
        after.accounts[index].id,
    )
        && crate::identity_model::parents_equal(
            before.accounts[index].parent_id,
            after.accounts[index].parent_id,
        )
        && crate::identity_model::revisions_equal(
            before.accounts[index].revision,
            after.accounts[index].revision,
        )
        && before.accounts[index].limits.spec_amounts().spec_equal(
            after.accounts[index].limits.spec_amounts(),
        )
}

pub(crate) open spec fn reservation_identity_stable(
    before: &BudgetLedger,
    after: &BudgetLedger,
    index: int,
) -> bool {
    requests_equal(
        before.reservations[index].request,
        after.reservations[index].request,
    )
}

pub(crate) open spec fn requests_equal(
    left: crate::BudgetRequest,
    right: crate::BudgetRequest,
) -> bool {
    crate::identity_model::reservation_ids_equal(
        left.spec_reservation_id(),
        right.spec_reservation_id(),
    )
        && crate::identity_model::budget_ids_equal(
            left.spec_budget_id(),
            right.spec_budget_id(),
        )
        && crate::identity_model::revisions_equal(
            left.spec_revision(),
            right.spec_revision(),
        )
        && crate::identity_model::action_ids_equal(
            left.spec_action_id(),
            right.spec_action_id(),
        )
        && crate::identity_model::digests_equal(
            left.spec_action_digest(),
            right.spec_action_digest(),
        )
        && left.spec_consume_now().spec_equal(right.spec_consume_now())
        && left.spec_reserve().spec_equal(right.spec_reserve())
}

pub(crate) open spec fn consumption_delta_equal(
    before_left: crate::state::BudgetAccount,
    after_left: crate::state::BudgetAccount,
    before_right: crate::state::BudgetAccount,
    after_right: crate::state::BudgetAccount,
) -> bool {
    after_left.consumed.spec_get(BudgetDimension::ModelTokens)
            - before_left.consumed.spec_get(BudgetDimension::ModelTokens)
        == after_right.consumed.spec_get(BudgetDimension::ModelTokens)
            - before_right.consumed.spec_get(BudgetDimension::ModelTokens)
        && after_left.consumed.spec_get(BudgetDimension::ProviderCostMicrounits)
                - before_left.consumed.spec_get(BudgetDimension::ProviderCostMicrounits)
            == after_right.consumed.spec_get(BudgetDimension::ProviderCostMicrounits)
                - before_right.consumed.spec_get(BudgetDimension::ProviderCostMicrounits)
        && after_left.consumed.spec_get(BudgetDimension::ActiveEffectMilliseconds)
                - before_left.consumed.spec_get(BudgetDimension::ActiveEffectMilliseconds)
            == after_right.consumed.spec_get(BudgetDimension::ActiveEffectMilliseconds)
                - before_right.consumed.spec_get(BudgetDimension::ActiveEffectMilliseconds)
        && after_left.consumed.spec_get(BudgetDimension::Attempts)
                - before_left.consumed.spec_get(BudgetDimension::Attempts)
            == after_right.consumed.spec_get(BudgetDimension::Attempts)
                - before_right.consumed.spec_get(BudgetDimension::Attempts)
        && after_left.consumed.spec_get(BudgetDimension::Retries)
                - before_left.consumed.spec_get(BudgetDimension::Retries)
            == after_right.consumed.spec_get(BudgetDimension::Retries)
                - before_right.consumed.spec_get(BudgetDimension::Retries)
}

pub(crate) open spec fn ancestor_consumption_propagates(
    before: &BudgetLedger,
    after: &BudgetLedger,
) -> bool {
    forall |child: int| #![auto]
        0 <= child < before.accounts@.len()
            && before.accounts[child].parent_id.is_some()
            && !before.accounts[child].consumed.spec_equal(after.accounts[child].consumed)
            ==> exists |parent: int| #![auto]
                0 <= parent < before.accounts@.len()
                    && crate::identity_model::parent_matches(
                        before.accounts[child].parent_id,
                        before.accounts[parent].id,
                    )
                    && consumption_delta_equal(
                        before.accounts[child],
                        after.accounts[child],
                        before.accounts[parent],
                        after.accounts[parent],
                    )
}

} // verus!
