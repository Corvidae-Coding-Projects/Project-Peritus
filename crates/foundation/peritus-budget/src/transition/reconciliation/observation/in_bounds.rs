//! In-ceiling interim and final usage application.

mod interim;

pub(in crate::transition) use self::interim::{apply_interim, apply_interim_validated};

use super::super::super::accounting::{
    charge_lineage, establish_observation_charge_safe, find_account, outstanding_validated,
    release_observation_charge, release_operation_reservation_validated, require_binding,
    require_reservation,
};
use crate::{
    BudgetAmounts, BudgetError, BudgetErrorKind, BudgetLedger, BudgetOperation, BudgetReceipt,
    BudgetReceiptKind, ReservationPhase, UsageFinality, UsageObservation,
};
use vstd::prelude::*;

verus! {

pub(in crate::transition) fn apply_final(
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
    let finality = observation.verified_finality();
    match finality {
        UsageFinality::Final => {}
        UsageFinality::Interim => {
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
    if !observation.verified_cumulative().fits_within(record.request.reserve()) {
        return Err(crate::model::corrupt(record.request.budget_id()));
    }
    if record.observed.equals(observation.verified_cumulative())
        && record.observation_evidence.is_some()
        && !crate::identity_model::optional_digest_equal(
            record.observation_evidence,
            Some(observation.verified_evidence_digest()),
        )
    {
        return Err(BudgetError::reservation(
            BudgetErrorKind::BindingMismatch,
            observation.verified_reservation_id(),
        ));
    }
    assert(ledger.reservations@[index as int].phase == phase);
    assert(observation.spec_finality() == finality);
    proof {
        assert(crate::reachability::observation_binding_guard(
            ledger.reservations[index as int],
            observation,
        ));
        crate::reachability::observation_guard_from_runtime(
            ledger,
            observation,
            BudgetReceiptKind::Applied,
            index as int,
        );
    }
    apply_final_validated(ledger, index, observation)
}

pub(in crate::transition) fn apply_final_validated(
    ledger: &mut BudgetLedger,
    reservation_index: usize,
    observation: UsageObservation,
) -> (result: Result<BudgetReceipt, BudgetError>)
    requires
        crate::reachability::accepted_guard(
            old(ledger),
            crate::BudgetCommand::ObserveUsage(observation),
            BudgetReceiptKind::Applied,
        ),
        crate::model::ledger_well_formed(old(ledger)),
        (reservation_index as int) < old(ledger).reservations@.len(),
        crate::identity_model::reservation_ids_equal(
            old(ledger).reservations[reservation_index as int]
                .request.spec_reservation_id(),
            observation.spec_reservation_id(),
        ),
        old(ledger).reservations[reservation_index as int].phase == ReservationPhase::Active,
        observation.spec_finality() == UsageFinality::Final,
        old(ledger).reservations[reservation_index as int]
            .observed.spec_le(observation.spec_cumulative()),
        observation.spec_cumulative().spec_le(
            old(ledger).reservations[reservation_index as int].request.spec_reserve(),
        ),
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
    let phase = ledger.reservations[reservation_index].phase;
    match phase {
        ReservationPhase::Active => {}
        _ => return Err(BudgetError::reservation(
            BudgetErrorKind::InvalidReservationPhase,
            observation.verified_reservation_id(),
        )),
    }
    let finality = observation.verified_finality();
    match finality {
        UsageFinality::Final => {}
        UsageFinality::Interim => return Err(BudgetError::reservation(
            BudgetErrorKind::InvalidReservationPhase,
            observation.verified_reservation_id(),
        )),
    }
    let record = &ledger.reservations[reservation_index];
    let high_water = record.observed;
    let cumulative = observation.verified_cumulative();
    let evidence = observation.verified_evidence_digest();
    let budget_id = record.request.budget_id();
    let ceiling = record.request.reserve();
    proof {
        assert(crate::invariant::reservation_entry_valid(
            &before,
            reservation_index as int,
        ));
    }
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
    let full_outstanding = outstanding_validated(record);
    let delta = match cumulative.checked_sub(high_water) {
        Ok(delta) => delta,
        Err(error) => return Err(BudgetError::arithmetic(error)),
    };
    establish_observation_charge_safe(ledger, reservation_index, cumulative, delta);
    release_observation_charge(ledger, reservation_index, delta);
    let ghost released_state = *ledger;
    charge_lineage(ledger, budget_id, delta)?;
    let ghost charged_state = *ledger;
    ledger.reservations[reservation_index].observed = cumulative;
    ledger.reservations[reservation_index].observation_evidence = Some(evidence);
    let released = match ceiling.checked_sub(cumulative) {
        Ok(released) => released,
        Err(error) => return Err(BudgetError::arithmetic(error)),
    };
    let ghost before_final_release = *ledger;
    proof {
        crate::accounting_model::reservation_outstanding_le_account(
            &before,
            reservation_index as int,
            account_index as int,
            full_outstanding,
        );
        crate::reachability::lineage_charge_preserves_account_id(
            &released_state,
            &charged_state,
            budget_id,
            delta,
            account_index as int,
        );
        assert(released.spec_le(
            before_final_release.accounts[account_index as int].operation_reserved,
        ));
    }
    release_operation_reservation_validated(ledger, account_index, budget_id, released);
    ledger.reservations[reservation_index].phase = ReservationPhase::SettledFinal;
    ledger.reservations[reservation_index].final_evidence = Some(evidence);
    ledger.reservations[reservation_index].final_reported = Some(cumulative);
    ledger.reservations[reservation_index].finality = Some(UsageFinality::Final);
    let receipt = BudgetReceipt::new(
        BudgetOperation::ObserveUsage,
        BudgetReceiptKind::Applied,
        budget_id,
        Some(observation.verified_reservation_id()),
        delta,
        released,
        Some(cumulative),
        Some(evidence),
    );
    proof {
        assert(before.reservations[reservation_index as int].phase == phase);
        assert(observation.spec_finality() == finality);
        assert(finality == UsageFinality::Final);
        assert(observation.spec_finality() == UsageFinality::Final);
        assert(crate::invariant::ledger_structure_holds(&before));
        assert(crate::invariant::reservation_entry_valid(
            &before,
            reservation_index as int,
        ));
        assert(crate::invariant::record_phase_valid(
            before.reservations[reservation_index as int],
        ));
        assert(BudgetAmounts::spec_difference(
            delta,
            observation.spec_cumulative(),
            before.reservations[reservation_index as int].observed,
        ));
        assert(BudgetAmounts::spec_difference(
            released,
            before.reservations[reservation_index as int].request.spec_reserve(),
            observation.spec_cumulative(),
        ));
        assert(ledger.reservations@ == before.reservations@.update(
            reservation_index as int,
            ledger.reservations[reservation_index as int],
        ));
        assert(crate::reachability::operation_release_exact(
            &before_final_release,
            ledger,
            budget_id,
            released,
        ));
        assert(crate::reachability::operation_release_exact(
            &charged_state,
            ledger,
            budget_id,
            released,
        ));
        assert(crate::reachability::observation_record_exact(
            &before,
            ledger,
            observation,
            observation.spec_cumulative(),
            ReservationPhase::SettledFinal,
            Some(observation.spec_cumulative()),
            Some(UsageFinality::Final),
        ));
        super::refinement::final_observation_refines(
            &before,
            ledger,
            &released_state,
            &charged_state,
            observation,
            receipt,
            budget_id,
            delta,
            released,
        );
    }
    Ok(receipt)
}

} // verus!
