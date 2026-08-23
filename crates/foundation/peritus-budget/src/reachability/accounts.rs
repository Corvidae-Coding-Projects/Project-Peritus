//! Exact account-vector effects used by the closed budget step relation.

#[cfg(verus_only)]
use crate::{BudgetAccountPhase, BudgetAmounts, BudgetDimension, BudgetLedger};
#[cfg(verus_only)]
use peritus_types::BudgetId;
use vstd::prelude::*;

verus! {

pub(crate) open spec fn amounts_balance(
    after: BudgetAmounts,
    before: BudgetAmounts,
    added: BudgetAmounts,
    removed: BudgetAmounts,
) -> bool {
    after.spec_get(BudgetDimension::ModelTokens)
            + removed.spec_get(BudgetDimension::ModelTokens)
        == before.spec_get(BudgetDimension::ModelTokens)
            + added.spec_get(BudgetDimension::ModelTokens)
        && after.spec_get(BudgetDimension::ProviderCostMicrounits)
                + removed.spec_get(BudgetDimension::ProviderCostMicrounits)
            == before.spec_get(BudgetDimension::ProviderCostMicrounits)
                + added.spec_get(BudgetDimension::ProviderCostMicrounits)
        && after.spec_get(BudgetDimension::ActiveEffectMilliseconds)
                + removed.spec_get(BudgetDimension::ActiveEffectMilliseconds)
            == before.spec_get(BudgetDimension::ActiveEffectMilliseconds)
                + added.spec_get(BudgetDimension::ActiveEffectMilliseconds)
        && after.spec_get(BudgetDimension::Attempts)
                + removed.spec_get(BudgetDimension::Attempts)
            == before.spec_get(BudgetDimension::Attempts)
                + added.spec_get(BudgetDimension::Attempts)
        && after.spec_get(BudgetDimension::Retries)
                + removed.spec_get(BudgetDimension::Retries)
            == before.spec_get(BudgetDimension::Retries)
                + added.spec_get(BudgetDimension::Retries)
}

pub(crate) open spec fn amounts_release_balance(
    after: BudgetAmounts,
    before: BudgetAmounts,
    charged: BudgetAmounts,
    released: BudgetAmounts,
) -> bool {
    after.spec_get(BudgetDimension::ModelTokens)
            + charged.spec_get(BudgetDimension::ModelTokens)
            + released.spec_get(BudgetDimension::ModelTokens)
        == before.spec_get(BudgetDimension::ModelTokens)
        && after.spec_get(BudgetDimension::ProviderCostMicrounits)
                + charged.spec_get(BudgetDimension::ProviderCostMicrounits)
                + released.spec_get(BudgetDimension::ProviderCostMicrounits)
            == before.spec_get(BudgetDimension::ProviderCostMicrounits)
        && after.spec_get(BudgetDimension::ActiveEffectMilliseconds)
                + charged.spec_get(BudgetDimension::ActiveEffectMilliseconds)
                + released.spec_get(BudgetDimension::ActiveEffectMilliseconds)
            == before.spec_get(BudgetDimension::ActiveEffectMilliseconds)
        && after.spec_get(BudgetDimension::Attempts)
                + charged.spec_get(BudgetDimension::Attempts)
                + released.spec_get(BudgetDimension::Attempts)
            == before.spec_get(BudgetDimension::Attempts)
        && after.spec_get(BudgetDimension::Retries)
                + charged.spec_get(BudgetDimension::Retries)
                + released.spec_get(BudgetDimension::Retries)
            == before.spec_get(BudgetDimension::Retries)
}

pub(crate) open spec fn immutable_account_fields_equal(
    before: crate::state::BudgetAccount,
    after: crate::state::BudgetAccount,
) -> bool {
    crate::identity_model::budget_ids_equal(before.id, after.id)
        && crate::identity_model::parents_equal(before.parent_id, after.parent_id)
        && crate::identity_model::revisions_equal(before.revision, after.revision)
        && before.limits.spec_amounts().spec_equal(after.limits.spec_amounts())
}

pub(crate) open spec fn account_exactly_equal(
    before: crate::state::BudgetAccount,
    after: crate::state::BudgetAccount,
) -> bool {
    immutable_account_fields_equal(before, after)
        && before.consumed.spec_equal(after.consumed)
        && before.operation_reserved.spec_equal(after.operation_reserved)
        && before.child_delegated_remaining.spec_equal(after.child_delegated_remaining)
        && before.phase == after.phase
}

pub(crate) open spec fn is_ancestor_index(
    ledger: &BudgetLedger,
    ancestor: int,
    descendant: int,
) -> bool
    decreases descendant,
{
    0 <= ancestor <= descendant < ledger.accounts@.len()
        && (ancestor == descendant
            || exists |parent: int| #![auto]
                0 <= parent < descendant
                    && crate::identity_model::parent_matches(
                        ledger.accounts[descendant].parent_id,
                        ledger.accounts[parent].id,
                    )
                    && is_ancestor_index(ledger, ancestor, parent))
}

pub(crate) open spec fn lineage_contains(
    ledger: &BudgetLedger,
    budget_id: BudgetId,
    account: int,
) -> bool {
    exists |target: int| #![auto]
        0 <= target < ledger.accounts@.len()
            && crate::identity_model::budget_ids_equal(ledger.accounts[target].id, budget_id)
            && is_ancestor_index(ledger, account, target)
}

pub(crate) open spec fn strict_lineage_ancestor(
    ledger: &BudgetLedger,
    budget_id: BudgetId,
    account: int,
) -> bool {
    lineage_contains(ledger, budget_id, account)
        && !crate::identity_model::budget_ids_equal(ledger.accounts[account].id, budget_id)
}

pub(crate) open spec fn begin_accounting_effect(
    before: &BudgetLedger,
    after: &BudgetLedger,
    budget_id: BudgetId,
    charged: BudgetAmounts,
    reserved: BudgetAmounts,
) -> bool {
    crate::identity_model::budget_ids_equal(before.root_id, after.root_id)
        && before.accounts@.len() == after.accounts@.len()
        && forall |index: int|
            0 <= index < before.accounts@.len() ==> {
                let prior = #[trigger] before.accounts[index];
                let next = after.accounts[index];
                immutable_account_fields_equal(prior, next)
                    && if lineage_contains(before, budget_id, index) {
                        BudgetAmounts::spec_sum(next.consumed, prior.consumed, charged)
                            && if crate::identity_model::budget_ids_equal(prior.id, budget_id) {
                                BudgetAmounts::spec_sum(
                                    next.operation_reserved,
                                    prior.operation_reserved,
                                    reserved,
                                ) && next.child_delegated_remaining.spec_equal(
                                    prior.child_delegated_remaining,
                                )
                            } else {
                                next.operation_reserved.spec_equal(prior.operation_reserved)
                                    && BudgetAmounts::spec_sum(
                                        prior.child_delegated_remaining,
                                        next.child_delegated_remaining,
                                        charged,
                                    )
                            }
                            && next.phase == prior.phase
                    } else {
                        account_exactly_equal(prior, next)
                    }
            }
}

pub(crate) open spec fn reservation_accounting_effect(
    before: &BudgetLedger,
    after: &BudgetLedger,
    budget_id: BudgetId,
    charged: BudgetAmounts,
    released: BudgetAmounts,
    fault_lineage: bool,
) -> bool {
    (!fault_lineage
        && exists |released_state: BudgetLedger| #![auto]
            super::account_updates::reservation_accounting(
                before,
                after,
                &released_state,
                budget_id,
                charged,
                released,
            ))
        || (fault_lineage
            && crate::identity_model::budget_ids_equal(before.root_id, after.root_id)
        && before.accounts@.len() == after.accounts@.len()
        && forall |index: int|
            0 <= index < before.accounts@.len() ==> {
                let prior = #[trigger] before.accounts[index];
                let next = after.accounts[index];
                immutable_account_fields_equal(prior, next)
                    && if lineage_contains(before, budget_id, index) {
                        BudgetAmounts::spec_sum(next.consumed, prior.consumed, charged)
                            && if crate::identity_model::budget_ids_equal(prior.id, budget_id) {
                                amounts_release_balance(
                                    next.operation_reserved,
                                    prior.operation_reserved,
                                    charged,
                                    released,
                                ) && next.child_delegated_remaining.spec_equal(
                                    prior.child_delegated_remaining,
                                )
                            } else {
                                next.operation_reserved.spec_equal(prior.operation_reserved)
                                    && BudgetAmounts::spec_sum(
                                        prior.child_delegated_remaining,
                                        next.child_delegated_remaining,
                                        charged,
                                    )
                            }
                            && next.phase == if fault_lineage {
                                BudgetAccountPhase::Faulted
                            } else {
                                prior.phase
                            }
                    } else {
                        account_exactly_equal(prior, next)
                    }
            })
}

pub(crate) open spec fn account_phase_effect(
    before: &BudgetLedger,
    after: &BudgetLedger,
    budget_id: BudgetId,
    phase: BudgetAccountPhase,
) -> bool {
    crate::identity_model::budget_ids_equal(before.root_id, after.root_id)
        && exists |index: int| #![auto]
            account_phase_record_effect(before, after, budget_id, phase, index)
}

pub(crate) open spec fn account_phase_record_effect(
    before: &BudgetLedger,
    after: &BudgetLedger,
    budget_id: BudgetId,
    phase: BudgetAccountPhase,
    index: int,
) -> bool {
    0 <= index < before.accounts@.len()
        && crate::identity_model::budget_ids_equal(before.accounts[index].id, budget_id)
        && after.accounts@ == before.accounts@.update(index, after.accounts[index])
        && {
            let prior = before.accounts[index];
            let next = after.accounts[index];
            immutable_account_fields_equal(prior, next)
                && prior.consumed.spec_equal(next.consumed)
                && prior.operation_reserved.spec_equal(next.operation_reserved)
                && prior.child_delegated_remaining.spec_equal(next.child_delegated_remaining)
                && next.phase == phase
        }
}

pub(crate) proof fn account_phase_effect_parts(
    before: &BudgetLedger,
    after: &BudgetLedger,
    budget_id: BudgetId,
    phase: BudgetAccountPhase,
)
    requires account_phase_effect(before, after, budget_id, phase),
    ensures
        crate::identity_model::budget_ids_equal(before.root_id, after.root_id),
        exists |index: int| #![auto]
            account_phase_record_effect(before, after, budget_id, phase, index),
{
}

pub(crate) proof fn account_phase_effect_from_record(
    before: &BudgetLedger,
    after: &BudgetLedger,
    budget_id: BudgetId,
    phase: BudgetAccountPhase,
    index: int,
)
    requires
        crate::identity_model::budget_ids_equal(before.root_id, after.root_id),
        account_phase_record_effect(before, after, budget_id, phase, index),
    ensures account_phase_effect(before, after, budget_id, phase),
{
    assert(exists |witness: int| #![auto]
        account_phase_record_effect(before, after, budget_id, phase, witness));
}

} // verus!
