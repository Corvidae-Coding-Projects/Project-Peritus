//! Interim in-ceiling usage application.

use crate::transition::accounting::{
    charge_lineage, establish_observation_charge_safe, release_observation_charge, require_binding,
    require_reservation,
};
use crate::{
    BudgetAmounts, BudgetError, BudgetErrorKind, BudgetLedger, BudgetOperation, BudgetReceipt,
    BudgetReceiptKind, ReservationPhase, UsageFinality, UsageObservation,
};
use vstd::prelude::*;

verus! {

pub(in crate::transition) fn apply_interim(
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
    super::super::super::super::validation::validate(ledger)?;
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
        UsageFinality::Interim => {}
        UsageFinality::Final => {
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
    let evidence = observation.verified_evidence_digest();
    if record.observed.equals(observation.verified_cumulative())
        && record.observation_evidence.is_some()
        && !crate::identity_model::optional_digest_equal(
            record.observation_evidence,
            Some(evidence),
        )
    {
        return Err(BudgetError::reservation(
            BudgetErrorKind::BindingMismatch,
            observation.verified_reservation_id(),
        ));
    }
    if record.observed.equals(observation.verified_cumulative())
        && crate::identity_model::optional_digest_equal(
            record.observation_evidence,
            Some(evidence),
        )
    {
        let receipt = BudgetReceipt::new(
            BudgetOperation::ObserveUsage,
            BudgetReceiptKind::Idempotent,
            record.request.budget_id(),
            Some(observation.verified_reservation_id()),
            BudgetAmounts::zero(),
            BudgetAmounts::zero(),
            Some(observation.verified_cumulative()),
            Some(evidence),
        );
        proof {
            crate::reachability::ledger_exact_reflexive(ledger);
            assert(crate::reachability::observation_binding_guard(
                ledger.reservations[index as int],
                observation,
            ));
            crate::reachability::observation_guard_from_runtime(
                ledger,
                observation,
                BudgetReceiptKind::Idempotent,
                index as int,
            );
            assert(receipt.spec_kind() == BudgetReceiptKind::Idempotent);
            assert(crate::reachability::observation_receipt_exact(
                receipt,
                observation,
                record.request.spec_budget_id(),
            ));
            crate::reachability::observation_refines(
                ledger,
                ledger,
                observation,
                receipt,
                record.request.spec_budget_id(),
            );
        }
        return Ok(receipt);
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
    apply_interim_validated(ledger, index, observation)
}

pub(in crate::transition) fn apply_interim_validated(
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
        observation.spec_finality() == UsageFinality::Interim,
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
        UsageFinality::Interim => {}
        UsageFinality::Final => return Err(BudgetError::reservation(
            BudgetErrorKind::InvalidReservationPhase,
            observation.verified_reservation_id(),
        )),
    }
    let record = &ledger.reservations[reservation_index];
    let high_water = record.observed;
    let cumulative = observation.verified_cumulative();
    let evidence = observation.verified_evidence_digest();
    let budget_id = record.request.budget_id();
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
    let receipt = BudgetReceipt::new(
        BudgetOperation::ObserveUsage,
        BudgetReceiptKind::Applied,
        budget_id,
        Some(observation.verified_reservation_id()),
        delta,
        BudgetAmounts::zero(),
        Some(cumulative),
        Some(evidence),
    );
    proof {
        assert(before.reservations[reservation_index as int].phase == phase);
        assert(observation.spec_finality() == finality);
        assert(finality == UsageFinality::Interim);
        assert(observation.spec_finality() == UsageFinality::Interim);
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
        assert(ledger.reservations@ == before.reservations@.update(
            reservation_index as int,
            ledger.reservations[reservation_index as int],
        ));
        assert(crate::refinement_model::requests_equal(
            before.reservations[reservation_index as int].request,
            ledger.reservations[reservation_index as int].request,
        ));
        assert(ledger.reservations[reservation_index as int].phase == phase);
        assert(crate::invariant::optional_digests_equal(
            before.reservations[reservation_index as int].activation_evidence,
            ledger.reservations[reservation_index as int].activation_evidence,
        ));
        assert(ledger.reservations[reservation_index as int].final_evidence
            == before.reservations[reservation_index as int].final_evidence);
        assert(ledger.reservations[reservation_index as int].final_reported
            == before.reservations[reservation_index as int].final_reported);
        assert(ledger.reservations[reservation_index as int].finality
            == before.reservations[reservation_index as int].finality);
        assert(ledger.reservations[reservation_index as int].final_evidence.is_none());
        assert(ledger.reservations[reservation_index as int].final_reported.is_none());
        assert(ledger.reservations[reservation_index as int].finality.is_none());
        assert(crate::reachability::observation_record_exact(
            &before,
            ledger,
            observation,
            observation.spec_cumulative(),
            ReservationPhase::Active,
            None,
            None,
        ));
        super::refinement::interim_observation_refines(
            &before,
            ledger,
            &released_state,
            &charged_state,
            observation,
            receipt,
            budget_id,
            delta,
        );
    }
    Ok(receipt)
}


} // verus!
