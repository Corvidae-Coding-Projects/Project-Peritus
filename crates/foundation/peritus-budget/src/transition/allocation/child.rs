//! Exact child-account allocation and idempotent replay.

use super::super::accounting::{
    find_account, receipt, require_open_lineage,
};
use crate::{
    BudgetAccountPhase, BudgetAmounts, BudgetError, BudgetErrorKind, BudgetLedger,
    BudgetOperation, BudgetReceipt, BudgetReceiptKind, ChildBudgetRequest,
};
use vstd::prelude::*;

verus! {

pub(in crate::transition) fn allocate_child(
    ledger: &mut BudgetLedger,
    request: ChildBudgetRequest,
) -> (result: Result<BudgetReceipt, BudgetError>)
    requires crate::model::ledger_well_formed(old(ledger)),
    ensures
        match result {
            Ok(receipt) => crate::reachability::candidate_step(
                old(ledger),
                crate::BudgetCommand::AllocateChild(request),
                final(ledger),
                receipt,
            ),
            Err(error) => crate::reachability::rejection_cause(
                old(ledger),
                crate::BudgetCommand::AllocateChild(request),
                error,
            ),
        },
{
    let ghost before = *ledger;
    let existing_account = find_account(ledger, request.verified_child_id());
    match existing_account {
        Some(existing_index) => {
            let existing = &ledger.accounts[existing_index];
            if crate::identity_model::parent_matches_id(
                existing.parent_id,
                request.verified_parent_id(),
            ) && crate::identity_model::revision_equal(
                existing.revision,
                request.verified_revision(),
            ) && existing
                .limits
                .amounts()
                .equals(request.verified_limits().amounts())
            {
                let replay = receipt(
                    BudgetOperation::AllocateChild,
                    BudgetReceiptKind::Idempotent,
                    request.verified_child_id(),
                );
                proof {
                    crate::reachability::allocate_guard_from_runtime(
                        ledger,
                        request,
                        BudgetReceiptKind::Idempotent,
                        existing_index as int,
                    );
                    assert(replay.spec_kind() == BudgetReceiptKind::Idempotent);
                    crate::reachability::allocate_idempotent_refines(ledger, request, replay);
                }
                return Ok(replay);
            }
            let error = BudgetError::budget(
                BudgetErrorKind::DuplicateBudgetConflict,
                request.verified_child_id(),
            );
            assert(crate::reachability::account_at_guard(
                ledger,
                request.spec_child_id(),
                existing_index as int,
            ));
            assert(crate::reachability::rejection_cause(
                ledger,
                crate::BudgetCommand::AllocateChild(request),
                error,
            ));
            return Err(error);
        }
        None => {}
    }
    assert(forall |index: int| #![auto]
        0 <= index < ledger.accounts@.len()
            ==> !crate::identity_model::budget_ids_equal(
                ledger.accounts[index].id,
                request.spec_child_id(),
            ));

    let parent_index = match require_open_lineage(ledger, request.verified_parent_id()) {
        Ok(parent_index) => parent_index,
        Err(error) => {
            assert(!crate::reachability::open_lineage_guard(
                ledger,
                request.spec_parent_id(),
            ));
            assert(crate::reachability::rejection_cause(
                ledger,
                crate::BudgetCommand::AllocateChild(request),
                error,
            ));
            return Err(error);
        }
    };
    assert(crate::reachability::account_at_guard(
        ledger,
        request.spec_parent_id(),
        parent_index as int,
    ));
    if !crate::identity_model::revision_equal(
        ledger.accounts[parent_index].revision,
        request.verified_revision(),
    ) {
        let error = BudgetError::budget(
            BudgetErrorKind::BindingMismatch,
            request.verified_parent_id(),
        );
        assert(crate::reachability::rejection_cause(
            ledger,
            crate::BudgetCommand::AllocateChild(request),
            error,
        ));
        return Err(error);
    }
    let available = match crate::model::available(&ledger.accounts[parent_index]) {
        Ok(available) => available,
        Err(error) => {
            proof {
                assert(crate::model::account_conserves(ledger.accounts[parent_index as int]));
                assert(false);
            }
            return Err(error);
        }
    };
    let requested = request.verified_limits().amounts();
    if !requested.fits_within(available) {
        let dimensions = requested.exceeding_dimensions(available);
        let error = BudgetError::insufficient(
            request.verified_parent_id(),
            dimensions,
        );
        assert(crate::reachability::rejection_cause(
            ledger,
            crate::BudgetCommand::AllocateChild(request),
            error,
        ));
        return Err(error);
    }
    proof {
        crate::reachability::capacity_from_available(
            ledger.accounts[parent_index as int],
            requested,
            available,
        );
    }
    ledger.accounts[parent_index].consumed.establish_bounds();
    ledger.accounts[parent_index].operation_reserved.establish_bounds();
    ledger.accounts[parent_index].child_delegated_remaining.establish_bounds();
    ledger.accounts[parent_index].limits.amounts().establish_bounds();
    requested.establish_bounds();
    let delegated = match ledger.accounts[parent_index]
        .child_delegated_remaining
        .checked_add(requested)
    {
        Ok(delegated) => delegated,
        Err(arithmetic) => {
            proof {
                assert(crate::reachability::capacity_guard(
                    ledger.accounts[parent_index as int],
                    requested,
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
    ledger.accounts.push(crate::state::BudgetAccount {
        id: request.verified_child_id(),
        parent_id: Some(request.verified_parent_id()),
        revision: request.verified_revision(),
        limits: request.verified_limits(),
        consumed: BudgetAmounts::zero(),
        operation_reserved: BudgetAmounts::zero(),
        child_delegated_remaining: BudgetAmounts::zero(),
        phase: BudgetAccountPhase::Open,
    });
    let applied_receipt = BudgetReceipt::new(
        BudgetOperation::AllocateChild,
        BudgetReceiptKind::Applied,
        request.verified_child_id(),
        None,
        BudgetAmounts::zero(),
        BudgetAmounts::zero(),
        None,
        None,
    );
    proof {
        assert(crate::identity_model::budget_ids_equal(before.root_id, ledger.root_id));
        assert(ledger.accounts@.len() == before.accounts@.len() + 1);
        assert(ledger.reservations@ == before.reservations@);
        assert(ledger.accounts@ == before.accounts@.update(
            parent_index as int,
            ledger.accounts[parent_index as int],
        ).push(ledger.accounts[before.accounts@.len() as int]));
        assert(crate::identity_model::budget_ids_equal(
            before.accounts[parent_index as int].id,
            request.spec_parent_id(),
        ));
        assert(BudgetAmounts::spec_sum(
            ledger.accounts[parent_index as int].child_delegated_remaining,
            before.accounts[parent_index as int].child_delegated_remaining,
            request.spec_limits().spec_amounts(),
        ));
        assert(crate::model::ledger_well_formed(&before));
        assert(forall |index: int| #![auto]
            0 <= index < before.accounts@.len()
                ==> !crate::identity_model::budget_ids_equal(
                    before.accounts[index].id,
                    request.spec_child_id(),
                ));
        assert(crate::reachability::open_lineage_guard(
            &before,
            request.spec_parent_id(),
        ));
        assert(crate::reachability::account_at_guard(
            &before,
            request.spec_parent_id(),
            parent_index as int,
        ));
        assert(crate::identity_model::revisions_equal(
            before.accounts[parent_index as int].revision,
            request.spec_revision(),
        ));
        assert(crate::reachability::capacity_guard(
            before.accounts[parent_index as int],
            request.spec_limits().spec_amounts(),
        ));
        assert(crate::reachability::child_allocation_exact(&before, ledger, request)) by {
            assert(exists |parent: int| #![auto]
                0 <= parent < before.accounts@.len()
                    && crate::identity_model::budget_ids_equal(
                        before.accounts[parent].id,
                        request.spec_parent_id(),
                    )
                    && ledger.accounts@ == before.accounts@.update(
                        parent,
                        ledger.accounts[parent],
                    ).push(ledger.accounts[before.accounts@.len() as int]));
        }
        crate::reachability::allocate_guard_from_runtime(
            &before,
            request,
            BudgetReceiptKind::Applied,
            parent_index as int,
        );
        assert(applied_receipt.spec_kind() == BudgetReceiptKind::Applied);
        crate::reachability::allocate_applied_refines(
            &before,
            ledger,
            request,
            applied_receipt,
        );
    }
    Ok(applied_receipt)
}

} // verus!
