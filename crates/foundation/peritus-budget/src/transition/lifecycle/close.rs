//! Account closure and release of unused delegated capacity.

use super::super::accounting::{find_account, has_live_work, receipt, require_account};
use crate::{
    BudgetAccountPhase, BudgetAmounts, BudgetError, BudgetErrorKind, BudgetLedger,
    BudgetOperation, BudgetReceipt, BudgetReceiptKind,
};
use peritus_types::BudgetId;
use vstd::prelude::*;

verus! {

pub(super) fn close(
    ledger: &mut BudgetLedger,
    budget_id: BudgetId,
) -> (result: Result<BudgetReceipt, BudgetError>)
    ensures
        match result {
            Ok(receipt) => crate::reachability::candidate_step(
                old(ledger),
                crate::BudgetCommand::Close(budget_id),
                final(ledger),
                receipt,
            ),
            Err(_) => true,
        },
{
    super::super::validation::validate(ledger)?;
    close_validated(ledger, budget_id)
}

pub(in crate::transition) fn close_validated(
    ledger: &mut BudgetLedger,
    budget_id: BudgetId,
) -> (result: Result<BudgetReceipt, BudgetError>)
    requires crate::model::ledger_well_formed(old(ledger)),
    ensures
        match result {
            Ok(receipt) => crate::reachability::candidate_step(
                old(ledger),
                crate::BudgetCommand::Close(budget_id),
                final(ledger),
                receipt,
            ),
            Err(error) => crate::reachability::rejection_cause(
                old(ledger),
                crate::BudgetCommand::Close(budget_id),
                error,
            ),
        },
{
    let ghost before = *ledger;
    proof {
        assert(crate::model::ledger_well_formed(&before));
    }
    let account_index = match require_account(ledger, budget_id) {
        Ok(index) => index,
        Err(error) => {
            assert(crate::reachability::rejection_cause(
                ledger,
                crate::BudgetCommand::Close(budget_id),
                error,
            ));
            return Err(error);
        }
    };
    let initial_phase = ledger.accounts[account_index].phase;
    match initial_phase {
        BudgetAccountPhase::Draining | BudgetAccountPhase::Faulted => {}
        BudgetAccountPhase::Open => {
            assert(crate::reachability::account_at_guard(
                &before,
                budget_id,
                account_index as int,
            ));
            let error = BudgetError::budget(BudgetErrorKind::InvalidAccountPhase, budget_id);
            assert(crate::reachability::rejection_cause(
                &before,
                crate::BudgetCommand::Close(budget_id),
                error,
            ));
            return Err(error);
        }
        BudgetAccountPhase::Closed => {
            let replay_receipt = receipt(
                BudgetOperation::Close,
                BudgetReceiptKind::Idempotent,
                budget_id,
            );
            proof {
                assert(before.accounts[account_index as int].phase == initial_phase);
                crate::reachability::ledger_exact_reflexive(ledger);
                assert(crate::reachability::ledgers_exactly_equal(&before, ledger));
                crate::reachability::lifecycle_guard_from_runtime(
                    &before,
                    budget_id,
                    BudgetReceiptKind::Idempotent,
                    true,
                    account_index as int,
                );
                assert(replay_receipt.spec_kind() == BudgetReceiptKind::Idempotent);
                crate::reachability::close_refines(
                    &before,
                    ledger,
                    budget_id,
                    replay_receipt,
                );
            }
            return Ok(replay_receipt);
        }
    }
    if has_live_work(ledger, budget_id) {
        assert(crate::reachability::account_at_guard(
            &before,
            budget_id,
            account_index as int,
        ));
        let error = BudgetError::budget(BudgetErrorKind::OutstandingWork, budget_id);
        assert(!crate::invariant::budget_has_no_live_work(&before, budget_id));
        assert(crate::reachability::rejection_cause(
            &before,
            crate::BudgetCommand::Close(budget_id),
            error,
        ));
        return Err(error);
    }
    ledger.accounts[account_index].limits.amounts().establish_bounds();
    ledger.accounts[account_index].consumed.establish_bounds();
    ledger.accounts[account_index].operation_reserved.establish_bounds();
    ledger.accounts[account_index]
        .child_delegated_remaining
        .establish_bounds();
    assert(crate::model::account_conserves(before.accounts[account_index as int]));
    assert(crate::model::account_balance_holds(
        before.accounts[account_index as int],
        crate::BudgetDimension::ModelTokens,
    ));
    assert(crate::model::account_balance_holds(
        before.accounts[account_index as int],
        crate::BudgetDimension::ProviderCostMicrounits,
    ));
    assert(crate::model::account_balance_holds(
        before.accounts[account_index as int],
        crate::BudgetDimension::ActiveEffectMilliseconds,
    ));
    assert(crate::model::account_balance_holds(
        before.accounts[account_index as int],
        crate::BudgetDimension::Attempts,
    ));
    assert(crate::model::account_balance_holds(
        before.accounts[account_index as int],
        crate::BudgetDimension::Retries,
    ));
    assert(before.accounts[account_index as int].consumed.spec_le(
        before.accounts[account_index as int].limits.spec_amounts(),
    ));
    let unused = match ledger.accounts[account_index]
        .limits
        .amounts()
        .checked_sub(ledger.accounts[account_index].consumed)
    {
        Ok(unused) => unused,
        Err(error) => {
            assert(BudgetAmounts::subtraction_error_exact(
                error,
                before.accounts[account_index as int].limits.spec_amounts(),
                before.accounts[account_index as int].consumed,
            ));
            assert(false);
            return Err(BudgetError::arithmetic(error));
        }
    };
    assert(BudgetAmounts::spec_difference(
        unused,
        before.accounts[account_index as int].limits.spec_amounts(),
        before.accounts[account_index as int].consumed,
    ));
    let parent_id = ledger.accounts[account_index].parent_id;
    let parent_index = match parent_id {
        Some(parent_id) => {
            let index = match find_account(ledger, parent_id) {
                Some(index) => index,
                None => {
                    proof {
                        assert(crate::invariant::ledger_structure_holds(&before));
                        assert(crate::invariant::account_entry_valid(
                            &before,
                            account_index as int,
                        ));
                        assert(crate::invariant::parent_link_valid(
                            &before,
                            account_index as int,
                        ));
                        let linked = choose |linked: int| #![auto]
                            0 <= linked < account_index
                                && crate::identity_model::parent_matches(
                                    before.accounts[account_index as int].parent_id,
                                    before.accounts[linked].id,
                                );
                        assert(crate::identity_model::budget_ids_equal(
                            before.accounts[linked].id,
                            parent_id,
                        ));
                        assert(false);
                    }
                    return Err(crate::model::corrupt(budget_id));
                }
            };
            Some(index)
        }
        None => None,
    };
    if let Some(parent_index) = parent_index {
        if parent_index == account_index {
            return Err(crate::model::corrupt(budget_id));
        }
        proof {
            assert(crate::identity_model::parent_matches(
                before.accounts[account_index as int].parent_id,
                before.accounts[parent_index as int].id,
            ));
            assert(crate::accounting_model::account_not_closed(
                before.accounts[account_index as int].phase,
            ));
        }
        crate::accounting_model::child_unused_le_parent(
            ledger,
            account_index,
            parent_index,
            unused,
        );
        ledger.accounts[parent_index]
            .child_delegated_remaining
            .establish_bounds();
        unused.establish_bounds();
        let delegated = match ledger.accounts[parent_index]
            .child_delegated_remaining
            .checked_sub(unused)
        {
            Ok(delegated) => delegated,
            Err(arithmetic) => {
                proof {
                    assert(unused.spec_le(
                        before.accounts[parent_index as int].child_delegated_remaining,
                    ));
                    match arithmetic.spec_dimension() {
                        crate::BudgetDimension::ModelTokens => assert(false),
                        crate::BudgetDimension::ProviderCostMicrounits => assert(false),
                        crate::BudgetDimension::ActiveEffectMilliseconds => assert(false),
                        crate::BudgetDimension::Attempts => assert(false),
                        crate::BudgetDimension::Retries => assert(false),
                    }
                }
                return Err(BudgetError::arithmetic(arithmetic));
            }
        };
        ledger.accounts[parent_index].child_delegated_remaining = delegated;
    }
    ledger.accounts[account_index].phase = BudgetAccountPhase::Closed;
    let close_receipt = BudgetReceipt::new(
        BudgetOperation::Close,
        BudgetReceiptKind::Applied,
        budget_id,
        None,
        BudgetAmounts::zero(),
        unused,
        None,
        None,
    );
    proof {
        assert(before.accounts[account_index as int].phase == initial_phase);
        assert(initial_phase == BudgetAccountPhase::Draining
            || initial_phase == BudgetAccountPhase::Faulted);
        prove_applied_close(
            &before,
            ledger,
            budget_id,
            account_index,
            parent_index,
            close_receipt,
        );
    }
    Ok(close_receipt)
}

proof fn prove_applied_close(
    before: &BudgetLedger,
    after: &BudgetLedger,
    budget_id: BudgetId,
    account_index: usize,
    parent_index: Option<usize>,
    close_receipt: BudgetReceipt,
)
    requires
        crate::model::ledger_well_formed(before),
        crate::identity_model::budget_ids_equal(before.root_id, after.root_id),
        before.reservations@ == after.reservations@,
        (account_index as int) < before.accounts@.len(),
        (account_index as int) < after.accounts@.len(),
        crate::identity_model::budget_ids_equal(
            before.accounts[account_index as int].id,
            budget_id,
        ),
        crate::reachability::immutable_account_fields_equal(
            before.accounts[account_index as int],
            after.accounts[account_index as int],
        ),
        before.accounts[account_index as int].consumed.spec_equal(
            after.accounts[account_index as int].consumed,
        ),
        before.accounts[account_index as int].operation_reserved.spec_equal(
            after.accounts[account_index as int].operation_reserved,
        ),
        before.accounts[account_index as int].child_delegated_remaining.spec_equal(
            after.accounts[account_index as int].child_delegated_remaining,
        ),
        before.accounts[account_index as int].phase == BudgetAccountPhase::Draining
            || before.accounts[account_index as int].phase == BudgetAccountPhase::Faulted,
        crate::invariant::budget_has_no_live_work(before, budget_id),
        after.accounts[account_index as int].phase == BudgetAccountPhase::Closed,
        crate::reachability::close_receipt_exact(close_receipt, budget_id),
        close_receipt.spec_kind() == BudgetReceiptKind::Applied,
        BudgetAmounts::spec_difference(
            close_receipt.spec_released(),
            before.accounts[account_index as int].limits.spec_amounts(),
            before.accounts[account_index as int].consumed,
        ),
        match parent_index {
            Some(parent_index) => {
                before.accounts[account_index as int].parent_id.is_some()
                    && (parent_index as int) < before.accounts@.len()
                    && (parent_index as int) < after.accounts@.len()
                    && after.accounts@ == before.accounts@.update(
                        parent_index as int,
                        after.accounts[parent_index as int],
                    ).update(account_index as int, after.accounts[account_index as int])
                    && crate::identity_model::parent_matches(
                        before.accounts[account_index as int].parent_id,
                        before.accounts[parent_index as int].id,
                    )
                    && crate::reachability::immutable_account_fields_equal(
                        before.accounts[parent_index as int],
                        after.accounts[parent_index as int],
                    )
                    && before.accounts[parent_index as int].consumed.spec_equal(
                        after.accounts[parent_index as int].consumed,
                    )
                    && before.accounts[parent_index as int].operation_reserved.spec_equal(
                        after.accounts[parent_index as int].operation_reserved,
                    )
                    && BudgetAmounts::spec_sum(
                        before.accounts[parent_index as int].child_delegated_remaining,
                        after.accounts[parent_index as int].child_delegated_remaining,
                        close_receipt.spec_released(),
                    )
                    && before.accounts[parent_index as int].phase
                        == after.accounts[parent_index as int].phase
            }
            None => {
                before.accounts[account_index as int].parent_id.is_none()
                    && after.accounts@ == before.accounts@.update(
                        account_index as int,
                        after.accounts[account_index as int],
                    )
            }
        },
    ensures
        crate::reachability::candidate_step(
            before,
            crate::BudgetCommand::Close(budget_id),
            after,
            close_receipt,
        ),
{
    assert(crate::reachability::close_effect_exact(
        before,
        after,
        budget_id,
        close_receipt,
    ));
    crate::reachability::lifecycle_guard_from_runtime(
        before,
        budget_id,
        BudgetReceiptKind::Applied,
        true,
        account_index as int,
    );
    crate::reachability::close_refines(before, after, budget_id, close_receipt);
}

} // verus!
