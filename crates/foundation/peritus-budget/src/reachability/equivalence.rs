//! Extensional transport across executable ledger duplication.

#[cfg(verus_only)]
use crate::{BudgetDimension, BudgetLedger};
use vstd::prelude::*;

verus! {

proof fn child_sum_equal(
    left: &BudgetLedger,
    right: &BudgetLedger,
    parent_id: peritus_types::BudgetId,
    dimension: BudgetDimension,
    end: int,
)
    requires
        left.accounts@ == right.accounts@,
        0 <= end <= left.accounts@.len(),
    ensures
        crate::accounting_model::direct_child_remaining_sum(
            left, parent_id, dimension, end,
        ) == crate::accounting_model::direct_child_remaining_sum(
            right, parent_id, dimension, end,
        ),
    decreases end,
{
    if end > 0 {
        child_sum_equal(left, right, parent_id, dimension, end - 1);
        assert(left.accounts[end - 1] == right.accounts[end - 1]);
        assert(crate::accounting_model::child_remaining_contribution(
            left,
            parent_id,
            dimension,
            end - 1,
        ) == crate::accounting_model::child_remaining_contribution(
            right,
            parent_id,
            dimension,
            end - 1,
        ));
    }
}

proof fn reservation_sum_equal(
    left: &BudgetLedger,
    right: &BudgetLedger,
    budget_id: peritus_types::BudgetId,
    dimension: BudgetDimension,
    end: int,
)
    requires
        left.reservations@ == right.reservations@,
        0 <= end <= left.reservations@.len(),
    ensures
        crate::accounting_model::direct_operation_reserved_sum(
            left, budget_id, dimension, end,
        ) == crate::accounting_model::direct_operation_reserved_sum(
            right, budget_id, dimension, end,
        ),
    decreases end,
{
    if end > 0 {
        reservation_sum_equal(left, right, budget_id, dimension, end - 1);
        assert(left.reservations[end - 1] == right.reservations[end - 1]);
        assert(crate::accounting_model::reservation_contribution(
            left,
            budget_id,
            dimension,
            end - 1,
        ) == crate::accounting_model::reservation_contribution(
            right,
            budget_id,
            dimension,
            end - 1,
        ));
    }
}

proof fn derived_accounting_transfers(
    left: &BudgetLedger,
    right: &BudgetLedger,
    index: int,
)
    requires
        left.accounts@ == right.accounts@,
        left.reservations@ == right.reservations@,
        0 <= index < left.accounts@.len(),
        crate::invariant::exact_derived_accounting(left, index),
    ensures crate::invariant::exact_derived_accounting(right, index),
{
    let budget_id = left.accounts[index].id;
    child_sum_equal(
        left, right, budget_id, BudgetDimension::ModelTokens, left.accounts@.len() as int,
    );
    child_sum_equal(
        left,
        right,
        budget_id,
        BudgetDimension::ProviderCostMicrounits,
        left.accounts@.len() as int,
    );
    child_sum_equal(
        left,
        right,
        budget_id,
        BudgetDimension::ActiveEffectMilliseconds,
        left.accounts@.len() as int,
    );
    child_sum_equal(
        left, right, budget_id, BudgetDimension::Attempts, left.accounts@.len() as int,
    );
    child_sum_equal(
        left, right, budget_id, BudgetDimension::Retries, left.accounts@.len() as int,
    );
    reservation_sum_equal(
        left,
        right,
        budget_id,
        BudgetDimension::ModelTokens,
        left.reservations@.len() as int,
    );
    reservation_sum_equal(
        left,
        right,
        budget_id,
        BudgetDimension::ProviderCostMicrounits,
        left.reservations@.len() as int,
    );
    reservation_sum_equal(
        left,
        right,
        budget_id,
        BudgetDimension::ActiveEffectMilliseconds,
        left.reservations@.len() as int,
    );
    reservation_sum_equal(
        left,
        right,
        budget_id,
        BudgetDimension::Attempts,
        left.reservations@.len() as int,
    );
    reservation_sum_equal(
        left,
        right,
        budget_id,
        BudgetDimension::Retries,
        left.reservations@.len() as int,
    );
}

proof fn account_entry_transfers(
    left: &BudgetLedger,
    right: &BudgetLedger,
    index: int,
)
    requires
        left.accounts@ == right.accounts@,
        left.reservations@ == right.reservations@,
        0 <= index < left.accounts@.len(),
        crate::invariant::account_entry_valid(left, index),
    ensures crate::invariant::account_entry_valid(right, index),
{
    derived_accounting_transfers(left, right, index);
    assert(crate::invariant::account_unique_before(right, index));
    if index > 0 {
        assert(crate::invariant::parent_link_valid(right, index));
    }
    if right.accounts[index].phase == crate::BudgetAccountPhase::Closed {
        assert(crate::invariant::closed_account_has_no_live_work(right, index)) by {
            assert forall |child: int| #![auto]
                0 <= child < right.accounts@.len()
                    && crate::identity_model::parent_matches(
                        right.accounts[child].parent_id,
                        right.accounts[index].id,
                    ) implies right.accounts[child].phase == crate::BudgetAccountPhase::Closed by {
                assert(left.accounts[child] == right.accounts[child]);
            }
            assert forall |reservation: int| #![auto]
                0 <= reservation < right.reservations@.len()
                    && crate::identity_model::budget_ids_equal(
                        right.reservations[reservation].request.spec_budget_id(),
                        right.accounts[index].id,
                    ) implies !right.reservations[reservation].phase.spec_is_live() by {
                assert(left.reservations[reservation] == right.reservations[reservation]);
            }
        }
    }
}

proof fn reservation_entry_transfers(
    left: &BudgetLedger,
    right: &BudgetLedger,
    index: int,
)
    requires
        left.accounts@ == right.accounts@,
        left.reservations@ == right.reservations@,
        0 <= index < left.reservations@.len(),
        crate::invariant::reservation_entry_valid(left, index),
    ensures crate::invariant::reservation_entry_valid(right, index),
{
    assert(left.reservations[index] == right.reservations[index]);
}

pub(crate) proof fn well_formed_transfers(
    source: &BudgetLedger,
    duplicate: &BudgetLedger,
)
    requires
        crate::model::ledger_well_formed(source),
        super::exact_input_view(source, duplicate),
    ensures crate::model::ledger_well_formed(duplicate),
{
    assert forall |index: int| #![auto]
        0 <= index < duplicate.accounts@.len()
            implies crate::model::account_conserves(duplicate.accounts[index]) by {
        assert(duplicate.accounts[index] == source.accounts[index]);
    }
    assert(crate::model::ledger_conserves(duplicate));
    assert(crate::invariant::ledger_account_structure_holds(duplicate)) by {
        assert(crate::identity_model::budget_ids_equal(
            duplicate.accounts[0].id,
            duplicate.root_id,
        ));
        assert forall |index: int| #![auto]
            0 <= index < duplicate.accounts@.len()
                implies crate::invariant::account_entry_valid(duplicate, index) by {
            account_entry_transfers(source, duplicate, index);
        }
    }
    assert(crate::invariant::ledger_reservation_structure_holds(duplicate)) by {
        assert forall |index: int| #![auto]
            0 <= index < duplicate.reservations@.len()
                implies crate::invariant::reservation_entry_valid(duplicate, index) by {
            reservation_entry_transfers(source, duplicate, index);
        }
    }
}

pub(crate) proof fn complete_refinement_source_transfers(
    source: &BudgetLedger,
    equivalent: &BudgetLedger,
    after: &BudgetLedger,
)
    requires
        super::exact_input_view(source, equivalent),
        super::complete_refinement(equivalent, after),
    ensures super::complete_refinement(source, after),
{
    assert(crate::model::ledger_consumption_monotonic(source, after)) by {
        assert forall |index: int| #![auto]
            0 <= index < source.accounts@.len()
                implies crate::model::consumption_monotonic(
                    source.accounts[index],
                    after.accounts[index],
                ) by {
            assert(source.accounts[index] == equivalent.accounts[index]);
        }
    }
    assert(crate::model::ledger_high_water_monotonic(source, after)) by {
        assert forall |index: int| #![auto]
            0 <= index < source.reservations@.len()
                implies crate::model::amounts_le(
                    source.reservations[index].observed,
                    after.reservations[index].observed,
                ) by {
            assert(source.reservations[index] == equivalent.reservations[index]);
        }
    }
    assert(crate::refinement_model::ledger_identity_stable(source, after)) by {
        assert(crate::refinement_model::ledger_identity_stable(equivalent, after));
        assert forall |index: int| #![auto]
            0 <= index < source.accounts@.len()
                implies crate::refinement_model::account_identity_stable(
                    source, after, index,
                ) by {
            assert(source.accounts[index] == equivalent.accounts[index]);
            assert(crate::refinement_model::account_identity_stable(
                equivalent,
                after,
                index,
            ));
        }
        assert forall |index: int| #![auto]
            0 <= index < source.reservations@.len()
                implies crate::refinement_model::reservation_identity_stable(
                    source, after, index,
                ) by {
            assert(source.reservations[index] == equivalent.reservations[index]);
            assert(crate::refinement_model::reservation_identity_stable(
                equivalent,
                after,
                index,
            ));
        }
    }
    assert(crate::refinement_model::ancestor_consumption_propagates(source, after)) by {
        assert forall |child: int| #![auto]
            0 <= child < source.accounts@.len()
                && source.accounts[child].parent_id.is_some()
                && !source.accounts[child].consumed.spec_equal(after.accounts[child].consumed)
                implies exists |parent: int| #![auto]
                    0 <= parent < source.accounts@.len()
                        && crate::identity_model::parent_matches(
                            source.accounts[child].parent_id,
                            source.accounts[parent].id,
                        )
                        && crate::refinement_model::consumption_delta_equal(
                            source.accounts[child],
                            after.accounts[child],
                            source.accounts[parent],
                            after.accounts[parent],
                        ) by {
            let parent = choose |parent: int| #![auto]
                0 <= parent < equivalent.accounts@.len()
                    && crate::identity_model::parent_matches(
                        equivalent.accounts[child].parent_id,
                        equivalent.accounts[parent].id,
                    )
                    && crate::refinement_model::consumption_delta_equal(
                        equivalent.accounts[child],
                        after.accounts[child],
                        equivalent.accounts[parent],
                        after.accounts[parent],
                    );
            assert(source.accounts[child] == equivalent.accounts[child]);
            assert(source.accounts[parent] == equivalent.accounts[parent]);
        }
    }
}

} // verus!
