//! Open-lineage validation and exact reservation-outstanding queries.

use super::require_account;
use crate::{
    BudgetAccountPhase, BudgetAmounts, BudgetError, BudgetErrorKind, BudgetLedger,
};
use peritus_types::BudgetId;
use vstd::prelude::*;

verus! {

pub(in crate::transition) fn require_open_lineage(
    ledger: &BudgetLedger,
    budget_id: BudgetId,
) -> (result: Result<usize, BudgetError>)
    requires crate::model::ledger_well_formed(ledger),
    ensures
        match result {
            Ok(index) => {
                (index as int) < ledger.accounts@.len()
                    && crate::identity_model::budget_ids_equal(
                        ledger.accounts[index as int].id,
                        budget_id,
                    )
                    && crate::reachability::open_lineage_guard(ledger, budget_id)
                    && ledger.accounts[index as int].phase == BudgetAccountPhase::Open
            }
            Err(error) => {
                crate::reachability::lineage_rejection(ledger, budget_id, error)
                    && !crate::reachability::open_lineage_guard(ledger, budget_id)
            }
        },
{
    let target = match require_account(ledger, budget_id) {
        Ok(index) => index,
        Err(error) => {
            proof {
                crate::reachability::absent_account_has_no_lineage(ledger, budget_id);
            }
            assert(!crate::reachability::open_lineage_guard(ledger, budget_id));
            assert(crate::reachability::lineage_rejection(ledger, budget_id, error));
            return Err(error);
        }
    };
    match require_open_parent_chain(ledger, target) {
        Ok(()) => {}
        Err(error) => {
            assert(crate::reachability::account_at_guard(
                ledger,
                budget_id,
                target as int,
            ));
            proof {
                crate::reachability::account_without_chain_has_no_lineage(
                    ledger,
                    budget_id,
                    target as int,
                );
            }
            assert(crate::reachability::lineage_rejection(ledger, budget_id, error));
            return Err(error);
        }
    }
    assert(crate::reachability::open_lineage_guard(ledger, budget_id));
    assert(ledger.accounts[target as int].phase == BudgetAccountPhase::Open);
    Ok(target)
}

fn require_open_parent_chain(
    ledger: &BudgetLedger,
    index: usize,
) -> (result: Result<(), BudgetError>)
    requires
        crate::model::ledger_well_formed(ledger),
        (index as int) < ledger.accounts@.len(),
    ensures
        match result {
            Ok(()) => crate::reachability::open_parent_chain_guard(ledger, index as int),
            Err(error) => {
                crate::reachability::first_non_open_account(
                    ledger,
                    index as int,
                    error,
                ) && !crate::reachability::open_parent_chain_guard(ledger, index as int)
            }
        },
    decreases index,
{
    let account = ledger.accounts[index];
    match account.phase {
        BudgetAccountPhase::Open => {}
        BudgetAccountPhase::Draining
        | BudgetAccountPhase::Faulted
        | BudgetAccountPhase::Closed => {
            proof {
                crate::reachability::non_open_head_has_no_chain(ledger, index as int);
            }
            let error = BudgetError::budget(BudgetErrorKind::AccountNotOpen, account.id);
            assert(crate::reachability::first_non_open_account(
                ledger,
                index as int,
                error,
            ));
            return Err(error);
        }
    }
    assert(account.phase == BudgetAccountPhase::Open);
    assert(ledger.accounts[index as int].phase == BudgetAccountPhase::Open);
    match account.parent_id {
        None => {
            assert(ledger.accounts[index as int].parent_id.is_none());
            proof {
                crate::reachability::open_chain_root(ledger, index as int);
            }
        }
        Some(parent_id) => {
            let parent_index = match require_account(ledger, parent_id) {
                Ok(parent_index) => parent_index,
                Err(error) => {
                    proof {
                        assert(crate::invariant::ledger_structure_holds(ledger));
                        assert(crate::invariant::account_entry_valid(ledger, index as int));
                        assert(crate::invariant::parent_link_valid(ledger, index as int));
                        let linked_parent = choose |linked_parent: int| #![auto]
                            0 <= linked_parent < index
                                && crate::identity_model::parent_matches(
                                    ledger.accounts[index as int].parent_id,
                                    ledger.accounts[linked_parent].id,
                                )
                                && crate::identity_model::revisions_equal(
                                    ledger.accounts[index as int].revision,
                                    ledger.accounts[linked_parent].revision,
                                );
                        assert(crate::identity_model::budget_ids_equal(
                            ledger.accounts[linked_parent].id,
                            parent_id,
                        ));
                        assert(false);
                    }
                    return Err(error);
                }
            };
            proof {
                assert(crate::invariant::ledger_structure_holds(ledger));
                assert(crate::invariant::account_entry_valid(ledger, index as int));
                assert(crate::invariant::parent_link_valid(ledger, index as int));
                let linked_parent = choose |linked_parent: int| #![auto]
                    0 <= linked_parent < index
                        && crate::identity_model::parent_matches(
                            ledger.accounts[index as int].parent_id,
                            ledger.accounts[linked_parent].id,
                        )
                        && crate::identity_model::revisions_equal(
                            ledger.accounts[index as int].revision,
                            ledger.accounts[linked_parent].revision,
                        );
                assert(crate::identity_model::budget_ids_equal(
                    ledger.accounts[parent_index as int].id,
                    ledger.accounts[linked_parent].id,
                ));
                crate::invariant::matching_accounts_are_unique(
                    ledger,
                    parent_index as int,
                    linked_parent,
                );
                assert((parent_index as int) < index);
            }
            let parent_result = require_open_parent_chain(ledger, parent_index);
            match parent_result {
                Ok(()) => {}
                Err(error) => {
                    proof {
                        if crate::reachability::open_parent_chain_guard(
                            ledger,
                            index as int,
                        ) {
                            crate::reachability::open_chain_implies_parent_chain(
                                ledger,
                                index as int,
                                parent_index as int,
                            );
                            assert(false);
                        }
                    }
                    assert(crate::reachability::first_non_open_account(
                        ledger,
                        index as int,
                        error,
                    )) by {
                        assert(crate::reachability::first_non_open_account(
                            ledger,
                            parent_index as int,
                            error,
                        ));
                        assert(crate::reachability::account_at_guard(
                            ledger,
                            parent_id,
                            parent_index as int,
                        ));
                        assert(exists |parent: int| #![auto]
                            parent == parent_index as int
                                && crate::reachability::first_non_open_account(
                                    ledger,
                                    parent,
                                    error,
                                ));
                    }
                    return Err(error);
                }
            }
            proof {
                assert(crate::reachability::open_parent_chain_guard(
                    ledger,
                    parent_index as int,
                ));
                assert(crate::identity_model::parent_matches(
                    ledger.accounts[index as int].parent_id,
                    ledger.accounts[parent_index as int].id,
                ));
                assert(exists |parent: int| #![auto]
                    0 <= parent < index
                        && crate::identity_model::parent_matches(
                            ledger.accounts[index as int].parent_id,
                            ledger.accounts[parent].id,
                        )
                        && crate::reachability::open_parent_chain_guard(ledger, parent));
                crate::reachability::open_chain_from_parent(
                    ledger,
                    index as int,
                    parent_index as int,
                );
            }
        }
    }
    Ok(())
}

pub(in crate::transition) const fn outstanding(
    record: &crate::state::ReservationRecord,
) -> (result: Result<BudgetAmounts, BudgetError>)
    ensures
        match result {
            Ok(amount) => {
                amount.spec_get(crate::BudgetDimension::ModelTokens)
                        == crate::accounting_model::record_outstanding(
                            *record,
                            crate::BudgetDimension::ModelTokens,
                        )
                    && amount.spec_get(crate::BudgetDimension::ProviderCostMicrounits)
                        == crate::accounting_model::record_outstanding(
                            *record,
                            crate::BudgetDimension::ProviderCostMicrounits,
                        )
                    && amount.spec_get(crate::BudgetDimension::ActiveEffectMilliseconds)
                        == crate::accounting_model::record_outstanding(
                            *record,
                            crate::BudgetDimension::ActiveEffectMilliseconds,
                        )
                    && amount.spec_get(crate::BudgetDimension::Attempts)
                        == crate::accounting_model::record_outstanding(
                            *record,
                            crate::BudgetDimension::Attempts,
                        )
                    && amount.spec_get(crate::BudgetDimension::Retries)
                        == crate::accounting_model::record_outstanding(
                            *record,
                            crate::BudgetDimension::Retries,
                        )
            }
            Err(error) => {
                record.phase.spec_is_live()
                    && !record.observed.spec_le(record.request.spec_reserve())
                    && crate::reachability::infrastructure_error(error)
            }
        },
{
    let live = record.phase.is_live();
    proof {
        if live {
            assert(record.phase.spec_is_live());
        } else {
            assert(!record.phase.spec_is_live());
        }
    }
    let amount = if live {
        record.request.reserve().establish_bounds();
        record.observed.establish_bounds();
        match record.request.reserve().checked_sub(record.observed) {
            Ok(amount) => {
                assert(BudgetAmounts::spec_difference(
                    amount,
                    record.request.spec_reserve(),
                    record.observed,
                ));
                amount
            }
            Err(error) => {
                proof {
                    match error.spec_dimension() {
                        crate::BudgetDimension::ModelTokens => {
                            assert(!record.observed.spec_le(record.request.spec_reserve()));
                        }
                        crate::BudgetDimension::ProviderCostMicrounits => {
                            assert(!record.observed.spec_le(record.request.spec_reserve()));
                        }
                        crate::BudgetDimension::ActiveEffectMilliseconds => {
                            assert(!record.observed.spec_le(record.request.spec_reserve()));
                        }
                        crate::BudgetDimension::Attempts => {
                            assert(!record.observed.spec_le(record.request.spec_reserve()));
                        }
                        crate::BudgetDimension::Retries => {
                            assert(!record.observed.spec_le(record.request.spec_reserve()));
                        }
                    }
                }
                return Err(BudgetError::arithmetic(error));
            }
        }
    } else {
        let zero = BudgetAmounts::zero();
        assert(zero.spec_is_zero());
        zero
    };
    Ok(amount)
}

pub(in crate::transition) const fn outstanding_validated(
    record: &crate::state::ReservationRecord,
) -> (amount: BudgetAmounts)
    requires
        record.phase.spec_is_live(),
        record.observed.spec_le(record.request.spec_reserve()),
    ensures BudgetAmounts::spec_difference(
        amount,
        record.request.spec_reserve(),
        record.observed,
    ),
{
    match outstanding(record) {
        Ok(amount) => amount,
        Err(_) => {
            assert(false);
            BudgetAmounts::zero()
        }
    }
}

} // verus!
