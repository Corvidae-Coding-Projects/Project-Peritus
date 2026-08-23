//! Exact child-allocation and closure account effects.

#[cfg(verus_only)]
use crate::{BudgetAccountPhase, BudgetAmounts, BudgetLedger, ChildBudgetRequest};
#[cfg(verus_only)]
use peritus_types::BudgetId;
use vstd::prelude::*;

verus! {

pub(crate) open spec fn child_allocation_effect(
    before: &BudgetLedger,
    after: &BudgetLedger,
    request: ChildBudgetRequest,
) -> bool {
    crate::identity_model::budget_ids_equal(before.root_id, after.root_id)
        && after.accounts@.len() == before.accounts@.len() + 1
        && after.reservations@ == before.reservations@
        && exists |parent: int| #![auto]
            0 <= parent < before.accounts@.len()
                && crate::identity_model::budget_ids_equal(
                    before.accounts[parent].id,
                    request.spec_parent_id(),
                )
                && after.accounts@ == before.accounts@.update(
                    parent,
                    after.accounts[parent],
                ).push(after.accounts[before.accounts@.len() as int])
                && {
                    let prior = before.accounts[parent];
                    let next = after.accounts[parent];
                    super::accounts::immutable_account_fields_equal(prior, next)
                        && prior.consumed.spec_equal(next.consumed)
                        && prior.operation_reserved.spec_equal(next.operation_reserved)
                        && BudgetAmounts::spec_sum(
                            next.child_delegated_remaining,
                            prior.child_delegated_remaining,
                            request.spec_limits().spec_amounts(),
                        )
                        && prior.phase == next.phase
                }
                && {
                    let child = after.accounts[before.accounts@.len() as int];
                    crate::identity_model::budget_ids_equal(child.id, request.spec_child_id())
                        && crate::identity_model::parent_matches(
                            child.parent_id,
                            request.spec_parent_id(),
                        )
                        && crate::identity_model::revisions_equal(
                            child.revision,
                            request.spec_revision(),
                        )
                        && child.limits.spec_amounts().spec_equal(
                            request.spec_limits().spec_amounts(),
                        )
                        && child.consumed.spec_is_zero()
                        && child.operation_reserved.spec_is_zero()
                        && child.child_delegated_remaining.spec_is_zero()
                        && child.phase == BudgetAccountPhase::Open
                }
}

pub(crate) open spec fn close_account_effect(
    before: &BudgetLedger,
    after: &BudgetLedger,
    budget_id: BudgetId,
    released: BudgetAmounts,
) -> bool {
    crate::identity_model::budget_ids_equal(before.root_id, after.root_id)
        && before.reservations@ == after.reservations@
        && exists |target: int| #![auto]
            0 <= target < before.accounts@.len()
                && crate::identity_model::budget_ids_equal(
                    before.accounts[target].id,
                    budget_id,
                )
                && {
                    let prior = before.accounts[target];
                    let next = after.accounts[target];
                    super::accounts::immutable_account_fields_equal(prior, next)
                        && prior.consumed.spec_equal(next.consumed)
                        && BudgetAmounts::spec_difference(
                            released,
                            prior.limits.spec_amounts(),
                            prior.consumed,
                        )
                        && prior.operation_reserved.spec_equal(next.operation_reserved)
                        && prior.child_delegated_remaining.spec_equal(
                            next.child_delegated_remaining,
                        )
                        && next.phase == BudgetAccountPhase::Closed
                }
                && match before.accounts[target].parent_id {
                    None => after.accounts@ == before.accounts@.update(
                        target,
                        after.accounts[target],
                    ),
                    Some(_) => exists |parent: int| #![auto]
                        0 <= parent < before.accounts@.len()
                            && crate::identity_model::parent_matches(
                                before.accounts[target].parent_id,
                                before.accounts[parent].id,
                            )
                            && after.accounts@ == before.accounts@.update(
                                parent,
                                after.accounts[parent],
                            ).update(target, after.accounts[target])
                            && {
                                let prior = before.accounts[parent];
                                let next = after.accounts[parent];
                                super::accounts::immutable_account_fields_equal(prior, next)
                                    && prior.consumed.spec_equal(next.consumed)
                                    && prior.operation_reserved.spec_equal(
                                        next.operation_reserved,
                                    )
                                    && BudgetAmounts::spec_sum(
                                        prior.child_delegated_remaining,
                                        next.child_delegated_remaining,
                                        released,
                                    )
                                    && prior.phase == next.phase
                            },
                }
}

pub(crate) open spec fn close_target_record_effect(
    before: &BudgetLedger,
    after: &BudgetLedger,
    budget_id: BudgetId,
    released: BudgetAmounts,
    target: int,
) -> bool {
    0 <= target < before.accounts@.len()
        && crate::identity_model::budget_ids_equal(before.accounts[target].id, budget_id)
        && {
            let prior = before.accounts[target];
            let next = after.accounts[target];
            super::accounts::immutable_account_fields_equal(prior, next)
                && prior.consumed.spec_equal(next.consumed)
                && BudgetAmounts::spec_difference(
                    released, prior.limits.spec_amounts(), prior.consumed,
                )
                && prior.operation_reserved.spec_equal(next.operation_reserved)
                && prior.child_delegated_remaining.spec_equal(next.child_delegated_remaining)
                && next.phase == BudgetAccountPhase::Closed
        }
}

pub(crate) open spec fn close_parent_record_effect(
    before: &BudgetLedger,
    after: &BudgetLedger,
    parent: int,
    released: BudgetAmounts,
) -> bool {
    0 <= parent < before.accounts@.len()
        && {
            let prior = before.accounts[parent];
            let next = after.accounts[parent];
            super::accounts::immutable_account_fields_equal(prior, next)
                && prior.consumed.spec_equal(next.consumed)
                && prior.operation_reserved.spec_equal(next.operation_reserved)
                && BudgetAmounts::spec_sum(
                    prior.child_delegated_remaining,
                    next.child_delegated_remaining,
                    released,
                )
                && prior.phase == next.phase
        }
}

pub(crate) proof fn close_account_effect_parts(
    before: &BudgetLedger,
    after: &BudgetLedger,
    budget_id: BudgetId,
    released: BudgetAmounts,
)
    requires close_account_effect(before, after, budget_id, released),
    ensures
        crate::identity_model::budget_ids_equal(before.root_id, after.root_id),
        before.reservations@ == after.reservations@,
        exists |target: int| #![auto]
            close_target_record_effect(before, after, budget_id, released, target)
                && match before.accounts[target].parent_id {
                    None => after.accounts@ == before.accounts@.update(
                        target, after.accounts[target],
                    ),
                    Some(_) => exists |parent: int| #![auto]
                        0 <= parent < before.accounts@.len()
                            && crate::identity_model::parent_matches(
                            before.accounts[target].parent_id,
                            before.accounts[parent].id,
                        )
                            && close_parent_record_effect(before, after, parent, released)
                            && after.accounts@ == before.accounts@.update(
                                parent, after.accounts[parent],
                            ).update(target, after.accounts[target]),
                },
{
    reveal(close_account_effect);
    reveal(close_target_record_effect);
    reveal(close_parent_record_effect);
    let target = choose |target: int| #![auto]
        0 <= target < before.accounts@.len()
            && crate::identity_model::budget_ids_equal(
                before.accounts[target].id,
                budget_id,
            )
            && {
                let prior = before.accounts[target];
                let next = after.accounts[target];
                super::accounts::immutable_account_fields_equal(prior, next)
                    && prior.consumed.spec_equal(next.consumed)
                    && BudgetAmounts::spec_difference(
                        released,
                        prior.limits.spec_amounts(),
                        prior.consumed,
                    )
                    && prior.operation_reserved.spec_equal(next.operation_reserved)
                    && prior.child_delegated_remaining.spec_equal(
                        next.child_delegated_remaining,
                    )
                    && next.phase == BudgetAccountPhase::Closed
            }
            && match before.accounts[target].parent_id {
                None => after.accounts@ == before.accounts@.update(
                    target,
                    after.accounts[target],
                ),
                Some(_) => exists |parent: int| #![auto]
                    0 <= parent < before.accounts@.len()
                        && crate::identity_model::parent_matches(
                            before.accounts[target].parent_id,
                            before.accounts[parent].id,
                        )
                        && after.accounts@ == before.accounts@.update(
                            parent,
                            after.accounts[parent],
                        ).update(target, after.accounts[target])
                        && {
                            let prior = before.accounts[parent];
                            let next = after.accounts[parent];
                            super::accounts::immutable_account_fields_equal(prior, next)
                                && prior.consumed.spec_equal(next.consumed)
                                && prior.operation_reserved.spec_equal(next.operation_reserved)
                                && BudgetAmounts::spec_sum(
                                    prior.child_delegated_remaining,
                                    next.child_delegated_remaining,
                                    released,
                                )
                                && prior.phase == next.phase
                        },
            };
    assert(close_target_record_effect(
        before, after, budget_id, released, target,
    ));
    match before.accounts[target].parent_id {
        None => {}
        Some(_) => {
            let parent = choose |parent: int| #![auto]
                0 <= parent < before.accounts@.len()
                    && crate::identity_model::parent_matches(
                        before.accounts[target].parent_id,
                        before.accounts[parent].id,
                    )
                    && after.accounts@ == before.accounts@.update(
                        parent,
                        after.accounts[parent],
                    ).update(target, after.accounts[target])
                    && {
                        let prior = before.accounts[parent];
                        let next = after.accounts[parent];
                        super::accounts::immutable_account_fields_equal(prior, next)
                            && prior.consumed.spec_equal(next.consumed)
                            && prior.operation_reserved.spec_equal(next.operation_reserved)
                            && BudgetAmounts::spec_sum(
                                prior.child_delegated_remaining,
                                next.child_delegated_remaining,
                                released,
                            )
                            && prior.phase == next.phase
                    };
            assert(close_parent_record_effect(before, after, parent, released));
        }
    }
    assert(exists |witness: int| #![auto]
        witness == target
            && close_target_record_effect(before, after, budget_id, released, witness)
            && match before.accounts[witness].parent_id {
                None => after.accounts@ == before.accounts@.update(
                    witness, after.accounts[witness],
                ),
                Some(_) => exists |parent: int| #![auto]
                    0 <= parent < before.accounts@.len()
                        && crate::identity_model::parent_matches(
                            before.accounts[witness].parent_id,
                            before.accounts[parent].id,
                        )
                        && close_parent_record_effect(before, after, parent, released)
                        && after.accounts@ == before.accounts@.update(
                            parent, after.accounts[parent],
                        ).update(witness, after.accounts[witness]),
            });
}

} // verus!
