//! Validation and exact branch selection for operation begin.

use super::begin_apply::apply_new_begin;
use super::super::accounting::{find_reservation, require_open_lineage};
use super::super::lifecycle::{retry_required, validate_attempt_charge};
use crate::{
    BudgetAmounts, BudgetError, BudgetErrorKind, BudgetLedger, BudgetOperation, BudgetReceipt,
    BudgetReceiptKind, BudgetRequest,
};
use vstd::prelude::*;

verus! {

pub(in crate::transition) fn begin(
    ledger: &mut BudgetLedger,
    request: BudgetRequest,
) -> (result: Result<BudgetReceipt, BudgetError>)
    requires crate::model::ledger_well_formed(old(ledger)),
    ensures
        match result {
            Ok(receipt) => crate::reachability::candidate_step(
                old(ledger),
                crate::BudgetCommand::Begin(request),
                final(ledger),
                receipt,
            ),
            Err(error) => crate::reachability::rejection_cause(
                old(ledger),
                crate::BudgetCommand::Begin(request),
                error,
            ),
        },
{
    let existing_reservation = find_reservation(ledger, request.verified_reservation_id());
    match existing_reservation {
        Some(existing_index) => {
            let existing = &ledger.reservations[existing_index];
            if existing.request.matches(request) {
                let replay = BudgetReceipt::new(
                    BudgetOperation::Begin,
                    BudgetReceiptKind::Idempotent,
                    request.budget_id(),
                    Some(request.verified_reservation_id()),
                    BudgetAmounts::zero(),
                    BudgetAmounts::zero(),
                    None,
                    None,
                );
                proof {
                    crate::reachability::ledger_exact_reflexive(ledger);
                    assert(crate::reachability::begin_receipt_exact(replay, request));
                    crate::reachability::begin_guard_from_runtime(
                        ledger,
                        request,
                        BudgetReceiptKind::Idempotent,
                        existing_index as int,
                    );
                    assert(replay.spec_kind() == BudgetReceiptKind::Idempotent);
                    crate::reachability::begin_refines(ledger, ledger, request, replay);
                }
                return Ok(replay);
            }
            let error = BudgetError::reservation(
                BudgetErrorKind::DuplicateReservationConflict,
                request.verified_reservation_id(),
            );
            assert(crate::reachability::reservation_at_guard(
                ledger,
                request.spec_reservation_id(),
                existing_index as int,
            ));
            assert(!crate::refinement_model::requests_equal(
                ledger.reservations[existing_index as int].request,
                request,
            ));
            assert(crate::reachability::begin_rejection(ledger, request, error));
            assert(crate::reachability::rejection_cause(
                ledger,
                crate::BudgetCommand::Begin(request),
                error,
            ));
            return Err(error);
        }
        None => {}
    }
    assert(forall |index: int| #![auto]
        0 <= index < ledger.reservations@.len()
            ==> !crate::identity_model::reservation_ids_equal(
                ledger.reservations[index].request.spec_reservation_id(),
                request.spec_reservation_id(),
            ));

    let consume_now = request.verified_consume_now();
    consume_now.establish_bounds();
    let reserve = request.reserve();
    let requested = match consume_now.checked_add(reserve) {
        Ok(requested) => requested,
        Err(arithmetic) => {
            let error = BudgetError::arithmetic(arithmetic);
            assert(BudgetAmounts::addition_error_exact(
                arithmetic,
                request.spec_consume_now(),
                request.spec_reserve(),
            ));
            assert(BudgetAmounts::spec_addition_overflows(
                request.spec_consume_now(),
                request.spec_reserve(),
            ));
            assert(crate::reachability::begin_after_identity_rejection(
                ledger,
                request,
                error,
            ));
            assert(crate::reachability::begin_rejection(ledger, request, error));
            assert(crate::reachability::rejection_cause(
                ledger,
                crate::BudgetCommand::Begin(request),
                error,
            ));
            return Err(error);
        }
    };
    assert(BudgetAmounts::spec_sum(
        requested,
        request.spec_consume_now(),
        request.spec_reserve(),
    ));
    if requested.is_zero() {
        let error = BudgetError::reservation(
            BudgetErrorKind::EmptyRequest,
            request.verified_reservation_id(),
        );
        BudgetAmounts::zero_sum_has_zero_operands(
            requested,
            request.verified_consume_now(),
            request.reserve(),
        );
        assert(crate::reachability::rejection_cause(
            ledger,
            crate::BudgetCommand::Begin(request),
            error,
        ));
        return Err(error);
    }
    let account_index = match require_open_lineage(ledger, request.budget_id()) {
        Ok(account_index) => account_index,
        Err(error) => {
            assert(!crate::reachability::open_lineage_guard(
                ledger,
                request.spec_budget_id(),
            ));
            assert(crate::reachability::rejection_cause(
                ledger,
                crate::BudgetCommand::Begin(request),
                error,
            ));
            return Err(error);
        }
    };
    assert(crate::reachability::account_at_guard(
        ledger,
        request.spec_budget_id(),
        account_index as int,
    ));
    if !crate::identity_model::revision_equal(
        ledger.accounts[account_index].revision,
        request.verified_revision(),
    ) {
        let error = BudgetError::reservation(
            BudgetErrorKind::BindingMismatch,
            request.verified_reservation_id(),
        );
        assert(crate::reachability::rejection_cause(
            ledger,
            crate::BudgetCommand::Begin(request),
            error,
        ));
        return Err(error);
    }
    let retry = match retry_required(ledger, request, ledger.reservations.len()) {
        Ok(retry) => retry,
        Err(error) => {
            assert(crate::reachability::rejection_cause(
                ledger,
                crate::BudgetCommand::Begin(request),
                error,
            ));
            return Err(error);
        }
    };
    match validate_attempt_charge(request, retry) {
        Ok(()) => {}
        Err(error) => {
            assert(crate::reachability::rejection_cause(
                ledger,
                crate::BudgetCommand::Begin(request),
                error,
            ));
            return Err(error);
        }
    }
    let available = match crate::model::available(&ledger.accounts[account_index]) {
        Ok(available) => available,
        Err(error) => {
            proof {
                assert(crate::model::account_conserves(ledger.accounts[account_index as int]));
                assert(false);
            }
            return Err(error);
        }
    };
    if !requested.fits_within(available) {
        let dimensions = requested.exceeding_dimensions(available);
        let error = BudgetError::insufficient(
            request.budget_id(),
            dimensions,
        );
        assert(crate::reachability::rejection_cause(
            ledger,
            crate::BudgetCommand::Begin(request),
            error,
        ));
        return Err(error);
    }

    proof {
        crate::reachability::request_capacity_from_available(
            ledger.accounts[account_index as int],
            request,
            requested,
            available,
        );
        assert(forall |index: int| #![auto]
            0 <= index < ledger.reservations@.len()
                ==> !crate::identity_model::reservation_ids_equal(
                    ledger.reservations[index].request.spec_reservation_id(),
                    request.spec_reservation_id(),
                ));
        assert(!request.spec_consume_now().spec_is_zero()
            || !request.spec_reserve().spec_is_zero());
        assert(crate::reachability::open_lineage_guard(
            ledger,
            request.spec_budget_id(),
        ));
        assert(crate::invariant::prior_history_resolved(
            ledger,
            request,
            ledger.reservations@.len() as int,
        ));
        assert(retry == crate::invariant::prior_exact_request(
            ledger,
            request,
            ledger.reservations@.len() as int,
        ));
        assert(crate::invariant::attempt_charge_valid(request, retry));
        assert(crate::reachability::account_at_guard(
            ledger,
            request.spec_budget_id(),
            account_index as int,
        ));
        assert(crate::identity_model::revisions_equal(
            ledger.accounts[account_index as int].revision,
            request.spec_revision(),
        ));
        assert(crate::reachability::request_capacity_guard(
            ledger.accounts[account_index as int],
            request,
        ));
        crate::reachability::begin_guard_from_runtime(
            ledger,
            request,
            BudgetReceiptKind::Applied,
            account_index as int,
        );
    }
    request.reserve().establish_bounds();
    assert(crate::reachability::capacity_guard(
        ledger.accounts[account_index as int],
        request.spec_consume_now(),
    ));

    apply_new_begin(ledger, account_index, request)
}

} // verus!
