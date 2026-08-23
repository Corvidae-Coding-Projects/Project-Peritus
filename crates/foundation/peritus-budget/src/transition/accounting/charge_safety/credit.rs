//! Recursive executable witness for chargeable account-lineage credit.

#[cfg(verus_only)]
use super::predicates::lineage_charge_safe_fuel;
use crate::{BudgetAmounts, BudgetLedger};
use peritus_types::BudgetId;
use vstd::prelude::*;

verus! {

pub(super) fn establish_account_credit_safe(
    ledger: &BudgetLedger,
    index: usize,
    current_id: BudgetId,
    amount: BudgetAmounts,
    delegated_child: bool,
    available_credit: bool,
    fuel: usize,
)
    requires
        crate::model::ledger_well_formed(ledger),
        index < fuel <= ledger.accounts@.len(),
        crate::identity_model::budget_ids_equal(
            ledger.accounts[index as int].id,
            current_id,
        ),
        crate::accounting_model::account_not_closed(ledger.accounts[index as int].phase),
        if delegated_child {
            amount.spec_le(ledger.accounts[index as int].child_delegated_remaining)
        } else if available_credit {
            crate::reachability::capacity_guard(ledger.accounts[index as int], amount)
        } else {
            amount.spec_le(ledger.accounts[index as int].operation_reserved)
        },
    ensures lineage_charge_safe_fuel(
        ledger,
        current_id,
        amount,
        delegated_child,
        fuel as nat,
    ),
    decreases fuel,
{
    let account = ledger.accounts[index];
    account.limits.amounts().establish_bounds();
    account.consumed.establish_bounds();
    account.operation_reserved.establish_bounds();
    account.child_delegated_remaining.establish_bounds();
    amount.establish_bounds();
    proof {
        assert(crate::model::account_conserves(account));
        assert(!BudgetAmounts::spec_addition_overflows(account.consumed, amount));
    }
    match account.parent_id {
        None => {}
        Some(parent_id) => {
            let parent_index = match super::super::find_account(ledger, parent_id) {
                Some(parent) => parent,
                None => {
                    proof {
                        assert(crate::invariant::account_entry_valid(ledger, index as int));
                        assert(crate::invariant::parent_link_valid(ledger, index as int));
                        assert(false);
                    }
                    return;
                }
            };
            assert(parent_index < index) by {
                assert(crate::invariant::account_entry_valid(ledger, index as int));
                assert(crate::invariant::parent_link_valid(ledger, index as int));
                let linked = choose |linked: int| #![auto]
                    0 <= linked < index
                        && crate::identity_model::parent_matches(
                            ledger.accounts[index as int].parent_id,
                            ledger.accounts[linked].id,
                        );
                assert(crate::identity_model::budget_ids_equal(
                    ledger.accounts[parent_index as int].id,
                    ledger.accounts[linked].id,
                ));
                crate::invariant::matching_accounts_are_unique(
                    ledger,
                    parent_index as int,
                    linked,
                );
            }
            let unused = match account.limits.amounts().checked_sub(account.consumed) {
                Ok(unused) => unused,
                Err(arithmetic) => {
                    proof {
                        assert(crate::model::account_conserves(account));
                        match arithmetic.spec_dimension() {
                            crate::BudgetDimension::ModelTokens => assert(false),
                            crate::BudgetDimension::ProviderCostMicrounits => assert(false),
                            crate::BudgetDimension::ActiveEffectMilliseconds => assert(false),
                            crate::BudgetDimension::Attempts => assert(false),
                            crate::BudgetDimension::Retries => assert(false),
                        }
                    }
                    return;
                }
            };
            assert(amount.spec_le(unused)) by {
                assert(BudgetAmounts::spec_difference(
                    unused,
                    account.limits.spec_amounts(),
                    account.consumed,
                ));
                assert(crate::model::account_conserves(account));
                assert(amount.spec_get(crate::BudgetDimension::ModelTokens)
                    <= unused.spec_get(crate::BudgetDimension::ModelTokens));
                assert(amount.spec_get(crate::BudgetDimension::ProviderCostMicrounits)
                    <= unused.spec_get(crate::BudgetDimension::ProviderCostMicrounits));
                assert(amount.spec_get(crate::BudgetDimension::ActiveEffectMilliseconds)
                    <= unused.spec_get(crate::BudgetDimension::ActiveEffectMilliseconds));
                assert(amount.spec_get(crate::BudgetDimension::Attempts)
                    <= unused.spec_get(crate::BudgetDimension::Attempts));
                assert(amount.spec_get(crate::BudgetDimension::Retries)
                    <= unused.spec_get(crate::BudgetDimension::Retries));
            }
            crate::accounting_model::child_unused_le_parent(
                ledger,
                index,
                parent_index,
                unused,
            );
            assert(amount.spec_le(
                ledger.accounts[parent_index as int].child_delegated_remaining,
            ));
            assert(crate::accounting_model::account_not_closed(
                ledger.accounts[parent_index as int].phase,
            )) by {
                if ledger.accounts[parent_index as int].phase
                    == crate::BudgetAccountPhase::Closed
                {
                    assert(crate::invariant::account_entry_valid(
                        ledger,
                        parent_index as int,
                    ));
                    assert(crate::invariant::closed_account_has_no_live_work(
                        ledger,
                        parent_index as int,
                    ));
                    assert(crate::identity_model::parent_matches(
                        ledger.accounts[index as int].parent_id,
                        ledger.accounts[parent_index as int].id,
                    ));
                    assert(crate::accounting_model::account_not_closed(account.phase));
                    assert(false);
                }
            }
            establish_account_credit_safe(
                ledger,
                parent_index,
                parent_id,
                amount,
                true,
                false,
                fuel - 1,
            );
        }
    }
    assert forall |candidate: int| #![auto]
        crate::reachability::account_at_guard(ledger, current_id, candidate)
            implies candidate == index as int by {
        assert(crate::identity_model::budget_ids_equal(
            ledger.accounts[candidate].id,
            ledger.accounts[index as int].id,
        ));
        crate::invariant::matching_accounts_are_unique(ledger, candidate, index as int);
    }
    assert(crate::reachability::account_at_guard(
        ledger,
        current_id,
        index as int,
    ));
    assert(exists |candidate: int| #![auto]
        crate::reachability::account_at_guard(ledger, current_id, candidate));
    assert forall |candidate: int| #![auto]
        crate::reachability::account_at_guard(ledger, current_id, candidate)
            implies (!delegated_child
                    || amount.spec_le(ledger.accounts[candidate].child_delegated_remaining))
                && !BudgetAmounts::spec_addition_overflows(
                    ledger.accounts[candidate].consumed,
                    amount,
                )
                && match ledger.accounts[candidate].parent_id {
                    Some(parent_id) => {
                        lineage_charge_safe_fuel(
                            ledger,
                            parent_id,
                            amount,
                            true,
                            (fuel as nat - 1) as nat,
                        ) && (forall |parent_index: int| #![auto]
                            crate::reachability::account_at_guard(
                                ledger,
                                parent_id,
                                parent_index,
                            ) ==> parent_index < candidate)
                    }
                    None => true,
                } by {
        assert(candidate == index as int);
        match account.parent_id {
            Some(parent_id) => {
                assert(lineage_charge_safe_fuel(
                    ledger,
                    parent_id,
                    amount,
                    true,
                    (fuel - 1) as nat,
                ));
                assert((fuel - 1) as nat == (fuel as nat - 1) as nat);
                assert forall |parent_index: int| #![auto]
                    crate::reachability::account_at_guard(ledger, parent_id, parent_index)
                        implies parent_index < candidate by {
                    assert(candidate == index as int);
                    assert(crate::invariant::account_entry_valid(ledger, index as int));
                    assert(crate::invariant::parent_link_valid(ledger, index as int));
                    let linked = choose |linked: int| #![auto]
                        0 <= linked < index
                            && crate::identity_model::parent_matches(
                                ledger.accounts[index as int].parent_id,
                                ledger.accounts[linked].id,
                            );
                    assert(crate::identity_model::budget_ids_equal(
                        ledger.accounts[parent_index].id,
                        ledger.accounts[linked].id,
                    ));
                    crate::invariant::matching_accounts_are_unique(
                        ledger,
                        parent_index,
                        linked,
                    );
                }
            }
            None => {}
        }
    }
    assert(lineage_charge_safe_fuel(
        ledger,
        current_id,
        amount,
        delegated_child,
        fuel as nat,
    ));
}

} // verus!
