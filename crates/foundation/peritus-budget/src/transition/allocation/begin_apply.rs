//! Atomic accounting and reservation insertion for a validated begin.

use super::super::accounting::{charge_lineage, establish_available_charge_safe};
#[cfg(verus_only)]
use crate::BudgetAccountPhase;
use crate::{
    BudgetAmounts, BudgetError, BudgetLedger, BudgetOperation, BudgetReceipt, BudgetReceiptKind,
    BudgetRequest, ReservationPhase,
};
use vstd::prelude::*;

verus! {

pub(super) fn apply_new_begin(
    ledger: &mut BudgetLedger,
    account_index: usize,
    request: BudgetRequest,
) -> (result: Result<BudgetReceipt, BudgetError>)
    requires
        crate::model::ledger_well_formed(old(ledger)),
        (account_index as int) < old(ledger).accounts@.len(),
        crate::identity_model::budget_ids_equal(
            old(ledger).accounts[account_index as int].id,
            request.spec_budget_id(),
        ),
        old(ledger).accounts[account_index as int].phase == BudgetAccountPhase::Open,
        crate::reachability::capacity_guard(
            old(ledger).accounts[account_index as int],
            request.spec_consume_now(),
        ),
        crate::reachability::request_capacity_guard(
            old(ledger).accounts[account_index as int],
            request,
        ),
        crate::reachability::accepted_guard(
            old(ledger),
            crate::BudgetCommand::Begin(request),
            BudgetReceiptKind::Applied,
        ),
    ensures
        result.is_ok(),
        match result {
            Ok(receipt) => crate::reachability::candidate_step(
                old(ledger),
                crate::BudgetCommand::Begin(request),
                final(ledger),
                receipt,
            ),
            Err(_) => false,
        },
{
    let ghost before = *ledger;
    let consume_now = request.verified_consume_now();
    consume_now.establish_bounds();
    let starting_account = ledger.accounts[account_index];
    starting_account.limits.amounts().establish_bounds();
    starting_account.consumed.establish_bounds();
    starting_account.operation_reserved.establish_bounds();
    starting_account.child_delegated_remaining.establish_bounds();
    assert(crate::accounting_model::account_not_closed(
        ledger.accounts[account_index as int].phase,
    ));
    assert(crate::reachability::capacity_guard(
        ledger.accounts[account_index as int],
        consume_now,
    ));
    establish_available_charge_safe(
        ledger,
        account_index,
        request.budget_id(),
        consume_now,
    );
    charge_lineage(ledger, request.budget_id(), consume_now)?;
    let ghost charged_state = *ledger;
    let reserve = request.reserve();
    ledger.accounts[account_index].operation_reserved.establish_bounds();
    reserve.establish_bounds();
    proof {
        crate::reachability::lineage_charge_preserves_account_id(
            &before,
            &charged_state,
            request.spec_budget_id(),
            request.spec_consume_now(),
            account_index as int,
        );
        assert(before.accounts[account_index as int]
            .operation_reserved.spec_get(crate::BudgetDimension::ModelTokens)
            + request.spec_reserve().spec_get(crate::BudgetDimension::ModelTokens)
            <= before.accounts[account_index as int]
                .limits.spec_amounts().spec_get(crate::BudgetDimension::ModelTokens));
        assert(before.accounts[account_index as int]
            .operation_reserved.spec_get(crate::BudgetDimension::ProviderCostMicrounits)
            + request.spec_reserve().spec_get(crate::BudgetDimension::ProviderCostMicrounits)
            <= before.accounts[account_index as int]
                .limits.spec_amounts().spec_get(crate::BudgetDimension::ProviderCostMicrounits));
        assert(before.accounts[account_index as int]
            .operation_reserved.spec_get(crate::BudgetDimension::ActiveEffectMilliseconds)
            + request.spec_reserve().spec_get(crate::BudgetDimension::ActiveEffectMilliseconds)
            <= before.accounts[account_index as int]
                .limits.spec_amounts().spec_get(crate::BudgetDimension::ActiveEffectMilliseconds));
        assert(before.accounts[account_index as int]
            .operation_reserved.spec_get(crate::BudgetDimension::Attempts)
            + request.spec_reserve().spec_get(crate::BudgetDimension::Attempts)
            <= before.accounts[account_index as int]
                .limits.spec_amounts().spec_get(crate::BudgetDimension::Attempts));
        assert(before.accounts[account_index as int]
            .operation_reserved.spec_get(crate::BudgetDimension::Retries)
            + request.spec_reserve().spec_get(crate::BudgetDimension::Retries)
            <= before.accounts[account_index as int]
                .limits.spec_amounts().spec_get(crate::BudgetDimension::Retries));
        assert(charged_state.accounts[account_index as int]
            .operation_reserved.spec_get(crate::BudgetDimension::ModelTokens)
            + request.spec_reserve().spec_get(crate::BudgetDimension::ModelTokens)
            <= before.accounts[account_index as int]
                .limits.spec_amounts().spec_get(crate::BudgetDimension::ModelTokens));
        assert(charged_state.accounts[account_index as int]
            .operation_reserved.spec_get(crate::BudgetDimension::ProviderCostMicrounits)
            + request.spec_reserve().spec_get(crate::BudgetDimension::ProviderCostMicrounits)
            <= before.accounts[account_index as int]
                .limits.spec_amounts().spec_get(crate::BudgetDimension::ProviderCostMicrounits));
        assert(charged_state.accounts[account_index as int]
            .operation_reserved.spec_get(crate::BudgetDimension::ActiveEffectMilliseconds)
            + request.spec_reserve().spec_get(crate::BudgetDimension::ActiveEffectMilliseconds)
            <= before.accounts[account_index as int]
                .limits.spec_amounts().spec_get(crate::BudgetDimension::ActiveEffectMilliseconds));
        assert(charged_state.accounts[account_index as int]
            .operation_reserved.spec_get(crate::BudgetDimension::Attempts)
            + request.spec_reserve().spec_get(crate::BudgetDimension::Attempts)
            <= before.accounts[account_index as int]
                .limits.spec_amounts().spec_get(crate::BudgetDimension::Attempts));
        assert(charged_state.accounts[account_index as int]
            .operation_reserved.spec_get(crate::BudgetDimension::Retries)
            + request.spec_reserve().spec_get(crate::BudgetDimension::Retries)
            <= before.accounts[account_index as int]
                .limits.spec_amounts().spec_get(crate::BudgetDimension::Retries));
        assert(!BudgetAmounts::spec_addition_overflows(
            charged_state.accounts[account_index as int].operation_reserved,
            request.spec_reserve(),
        ));
    }
    ledger.accounts[account_index].operation_reserved = ledger.accounts[account_index]
        .operation_reserved
        .checked_add(reserve)
        .map_err(BudgetError::arithmetic)?;
    let phase = if reserve.is_zero() {
        ReservationPhase::SettledExact
    } else {
        ReservationPhase::Held
    };
    ledger.reservations.push(crate::state::ReservationRecord {
        request,
        observed: BudgetAmounts::zero(),
        phase,
        activation_evidence: None,
        observation_evidence: None,
        final_evidence: None,
        final_reported: None,
        finality: None,
    });
    let applied_receipt = BudgetReceipt::new(
        BudgetOperation::Begin,
        BudgetReceiptKind::Applied,
        request.budget_id(),
        Some(request.verified_reservation_id()),
        consume_now,
        BudgetAmounts::zero(),
        None,
        None,
    );
    proof {
        assert(crate::identity_model::budget_ids_equal(
            charged_state.accounts[account_index as int].id,
            request.spec_budget_id(),
        ));
        assert(crate::identity_model::budget_ids_equal(
            charged_state.root_id,
            ledger.root_id,
        ));
        assert(crate::reachability::reserved_account_exact(
            charged_state.accounts[account_index as int],
            ledger.accounts[account_index as int],
            reserve,
        ));
        assert(ledger.accounts@ == charged_state.accounts@.update(
            account_index as int,
            ledger.accounts[account_index as int],
        ));
        assert(crate::reachability::operation_reserve_exact(
            &charged_state,
            ledger,
            request.spec_budget_id(),
            request.spec_reserve(),
        ));
        assert(crate::reachability::begin_accounting_exact(
            &before,
            ledger,
            &charged_state,
            request.spec_budget_id(),
            request.spec_consume_now(),
            request.spec_reserve(),
        ));
        assert(ledger.reservations@ == before.reservations@.push(
            ledger.reservations[before.reservations@.len() as int],
        ));
        assert(crate::reachability::begin_record_exact(&before, ledger, request));
        assert(crate::reachability::begin_receipt_exact(applied_receipt, request));
        assert(crate::reachability::accepted_guard(
            &before,
            crate::BudgetCommand::Begin(request),
            applied_receipt.spec_kind(),
        ));
        crate::reachability::begin_refines(
            &before,
            ledger,
            request,
            applied_receipt,
        );
    }
    Ok(applied_receipt)
}

} // verus!
