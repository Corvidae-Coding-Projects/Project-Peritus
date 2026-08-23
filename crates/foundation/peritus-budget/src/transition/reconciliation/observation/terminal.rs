//! Overrun and exact terminal-observation reconciliation.

mod replay;

pub(in crate::transition) use self::replay::replay_final_observation_validated;

use super::super::super::accounting::{
    charge_lineage, establish_fault_lineage_safe, establish_reserved_charge_safe, fault_lineage,
    find_account, outstanding_validated, release_full_reservation, require_binding,
    require_reservation,
};
#[cfg(verus_only)]
use super::super::super::accounting::identity_stability_preserves_fault_safety;
use crate::{
    BudgetAmounts, BudgetError, BudgetErrorKind, BudgetLedger, BudgetOperation, BudgetReceipt,
    BudgetReceiptKind, ReservationPhase, UsageObservation,
};
use vstd::prelude::*;

verus! {

pub(in crate::transition) fn apply_overrun(
    ledger: &mut BudgetLedger,
    observation: UsageObservation,
) -> (result: Result<BudgetReceipt, BudgetError>)
    ensures
        match result {
            Ok(receipt) => crate::reachability::candidate_step(
                old(ledger),
                crate::BudgetCommand::ObserveUsage(observation),
                final(ledger),
                receipt,
            ),
            Err(_) => true,
        },
{
    super::super::super::validation::validate(ledger)?;
    let index = require_reservation(ledger, observation.verified_reservation_id())?;
    require_binding(
        &ledger.reservations[index],
        observation.verified_action_id(),
        observation.verified_action_digest(),
    )?;
    let phase = ledger.reservations[index].phase;
    match phase {
        ReservationPhase::Active => {}
        _ => {
            return Err(BudgetError::reservation(
                BudgetErrorKind::InvalidReservationPhase,
                observation.verified_reservation_id(),
            ));
        }
    }
    let record = &ledger.reservations[index];
    if !record.observed.fits_within(observation.verified_cumulative()) {
        return Err(BudgetError::reservation(
            BudgetErrorKind::NonmonotonicObservation,
            observation.verified_reservation_id(),
        ));
    }
    if observation.verified_cumulative().fits_within(record.request.reserve()) {
        return Err(crate::model::corrupt(record.request.budget_id()));
    }
    assert(ledger.reservations@[index as int].phase == phase);
    proof {
        assert(crate::reachability::observation_binding_guard(
            ledger.reservations[index as int],
            observation,
        ));
        crate::reachability::observation_guard_from_runtime(
            ledger,
            observation,
            BudgetReceiptKind::OverrunFaulted,
            index as int,
        );
    }
    apply_overrun_validated(ledger, index, observation)
}

pub(in crate::transition) fn apply_overrun_validated(
    ledger: &mut BudgetLedger,
    reservation_index: usize,
    observation: UsageObservation,
) -> (result: Result<BudgetReceipt, BudgetError>)
    requires
        crate::reachability::accepted_guard(
            old(ledger),
            crate::BudgetCommand::ObserveUsage(observation),
            BudgetReceiptKind::OverrunFaulted,
        ),
        crate::model::ledger_well_formed(old(ledger)),
        (reservation_index as int) < old(ledger).reservations@.len(),
        crate::identity_model::reservation_ids_equal(
            old(ledger).reservations[reservation_index as int]
                .request.spec_reservation_id(),
            observation.spec_reservation_id(),
        ),
        old(ledger).reservations[reservation_index as int].phase == ReservationPhase::Active,
    ensures
        result.is_ok(),
        match result {
            Ok(receipt) => crate::reachability::candidate_step(
                old(ledger),
                crate::BudgetCommand::ObserveUsage(observation),
                final(ledger),
                receipt,
            ),
            Err(_) => false,
        },
{
    let ghost before = *ledger;
    match ledger.reservations[reservation_index].phase {
        ReservationPhase::Active => {}
        _ => {
            return Err(BudgetError::reservation(
                BudgetErrorKind::InvalidReservationPhase,
                observation.verified_reservation_id(),
            ));
        }
    }
    let record = &ledger.reservations[reservation_index];
    let budget_id = record.request.budget_id();
    let ceiling = record.request.reserve();
    assert(crate::invariant::ledger_structure_holds(&before));
    assert(crate::invariant::reservation_entry_valid(
        &before,
        reservation_index as int,
    ));
    let account_index = match find_account(ledger, budget_id) {
        Some(index) => index,
        None => {
            proof {
                let account = choose |account: int| #![auto]
                    0 <= account < before.accounts@.len()
                        && crate::identity_model::budget_ids_equal(
                            before.reservations[reservation_index as int]
                                .request.spec_budget_id(),
                            before.accounts[account].id,
                        );
                assert(false);
            }
            return Err(crate::model::corrupt(budget_id));
        }
    };
    establish_fault_lineage_safe(
        ledger,
        account_index,
        budget_id,
        ledger.accounts.len(),
    );
    let remaining = outstanding_validated(record);
    BudgetAmounts::difference_le_left(
        remaining,
        record.request.reserve(),
        record.observed,
    );
    assert forall |dimension: crate::BudgetDimension| #![auto]
        remaining.spec_get(dimension)
            <= crate::accounting_model::record_outstanding(
                before.reservations[reservation_index as int],
                dimension,
            ) by {
        assert(remaining.spec_get(dimension)
            == crate::accounting_model::record_outstanding(
                before.reservations[reservation_index as int],
                dimension,
            ));
    }
    establish_reserved_charge_safe(ledger, reservation_index, remaining);
    release_full_reservation(ledger, reservation_index, remaining);
    let ghost released_state = *ledger;
    charge_lineage(ledger, budget_id, remaining)?;
    let ghost charged_state = *ledger;
    ledger.reservations[reservation_index].observed = ceiling;
    ledger.reservations[reservation_index].observation_evidence =
        Some(observation.verified_evidence_digest());
    ledger.reservations[reservation_index].phase = ReservationPhase::OverrunFaulted;
    ledger.reservations[reservation_index].final_evidence =
        Some(observation.verified_evidence_digest());
    ledger.reservations[reservation_index].final_reported =
        Some(observation.verified_cumulative());
    ledger.reservations[reservation_index].finality = Some(observation.verified_finality());
    let ghost before_fault = *ledger;
    proof {
        assert(before_fault.accounts@ == charged_state.accounts@);
        assert(before_fault.reservations@ == before.reservations@.update(
            reservation_index as int,
            before_fault.reservations[reservation_index as int],
        ));
        assert forall |index: int| #![auto]
            0 <= index < before.accounts@.len() implies
                crate::refinement_model::account_identity_stable(
                    &before,
                    &before_fault,
                    index,
                ) by {
            crate::reachability::operation_release_preserves_account_identity(
                &before,
                &released_state,
                budget_id,
                remaining,
                index,
            );
            crate::reachability::lineage_charge_preserves_account_id(
                &released_state,
                &charged_state,
                budget_id,
                remaining,
                index,
            );
        }
        assert forall |index: int| #![auto]
            0 <= index < before.reservations@.len() implies
                crate::refinement_model::reservation_identity_stable(
                    &before,
                    &before_fault,
                    index,
                ) by {
            if index == reservation_index as int {
                assert(before.reservations[index].request
                    == before_fault.reservations[index].request);
            } else {
                assert(before.reservations[index] == before_fault.reservations[index]);
            }
        }
        assert(crate::refinement_model::ledger_identity_stable(
            &before,
            &before_fault,
        ));
        identity_stability_preserves_fault_safety(&before, &before_fault, budget_id);
    }
    fault_lineage(ledger, budget_id)?;
    let overrun_receipt = make_overrun_receipt(observation, budget_id, remaining);
    proof {
        assert(crate::invariant::ledger_structure_holds(&before));
        assert(crate::invariant::reservation_entry_valid(
            &before,
            reservation_index as int,
        ));
        assert(crate::invariant::record_phase_valid(
            before.reservations[reservation_index as int],
        ));
        assert(before.reservations[reservation_index as int].phase
            == ReservationPhase::Active);
        assert(BudgetAmounts::spec_difference(
            remaining,
            before.reservations[reservation_index as int].request.spec_reserve(),
            before.reservations[reservation_index as int].observed,
        ));
        assert(crate::reachability::reservation_bound_to_budget(
            &before,
            observation.spec_reservation_id(),
            budget_id,
        ));
        assert(crate::reachability::observation_receipt_exact(
            overrun_receipt,
            observation,
            budget_id,
        ));
        assert(before_fault.accounts@ == charged_state.accounts@);
        assert(crate::reachability::lineage_fault_exact(
            &before_fault,
            ledger,
            budget_id,
        ));
        assert(crate::reachability::lineage_fault_exact(
            &charged_state,
            ledger,
            budget_id,
        ));
        assert(crate::reachability::overrun_accounting_exact(
            &before,
            ledger,
            &released_state,
            &charged_state,
            budget_id,
            remaining,
        ));
        assert(remaining.spec_equal(overrun_receipt.spec_charged()));
        assert(exists |exact_release_state: BudgetLedger, exact_charged_state: BudgetLedger,
            exact_charged: BudgetAmounts| #![auto]
            exact_charged.spec_equal(overrun_receipt.spec_charged())
                && crate::reachability::overrun_accounting_exact(
                    &before,
                    ledger,
                    &exact_release_state,
                    &exact_charged_state,
                    budget_id,
                    exact_charged,
                ));
        assert(ledger.reservations@ == before.reservations@.update(
            reservation_index as int,
            ledger.reservations[reservation_index as int],
        ));
        assert(crate::reachability::observation_record_exact(
            &before,
            ledger,
            observation,
            before.reservations[reservation_index as int].request.spec_reserve(),
            ReservationPhase::OverrunFaulted,
            Some(observation.spec_cumulative()),
            Some(observation.spec_finality()),
        ));
        assert(exists |index: int| #![auto]
            0 <= index < before.reservations@.len()
                && crate::identity_model::reservation_ids_equal(
                    before.reservations[index].request.spec_reservation_id(),
                    observation.spec_reservation_id(),
                )
                && before.reservations[index].phase == ReservationPhase::Active
                && BudgetAmounts::spec_difference(
                    overrun_receipt.spec_charged(),
                    before.reservations[index].request.spec_reserve(),
                    before.reservations[index].observed,
                ));
        crate::reachability::overrun_active_effect_exact(
            &before,
            ledger,
            observation,
            overrun_receipt,
            budget_id,
            reservation_index as int,
            &released_state,
            &charged_state,
            remaining,
        );
        assert(exists |index: int| #![auto]
            crate::reachability::observation_overrun_effect(
                &before, observation, ledger, overrun_receipt, budget_id, index,
            ));
        crate::reachability::observation_refines(
            &before,
            ledger,
            observation,
            overrun_receipt,
            budget_id,
        );
    }
    Ok(overrun_receipt)
}

const fn make_overrun_receipt(
    observation: UsageObservation,
    budget_id: peritus_types::BudgetId,
    charged: BudgetAmounts,
) -> (receipt: BudgetReceipt)
    ensures
        crate::reachability::observation_receipt_exact(receipt, observation, budget_id),
        receipt.spec_kind() == BudgetReceiptKind::OverrunFaulted,
        receipt.spec_charged().spec_equal(charged),
        receipt.spec_released().spec_is_zero(),
{
    BudgetReceipt::new(
        BudgetOperation::ObserveUsage,
        BudgetReceiptKind::OverrunFaulted,
        budget_id,
        Some(observation.verified_reservation_id()),
        charged,
        BudgetAmounts::zero(),
        Some(observation.verified_cumulative()),
        Some(observation.verified_evidence_digest()),
    )
}

} // verus!
