//! Monotonic high-water observation transitions.

use super::super::accounting::{
    require_binding, require_reservation,
};
use crate::{
    BudgetAmounts, BudgetError, BudgetErrorKind, BudgetLedger, BudgetOperation,
    BudgetReceipt, BudgetReceiptKind, ReservationPhase, UsageFinality, UsageObservation,
};
use vstd::prelude::*;

mod terminal;
mod refinement;
mod in_bounds;

use in_bounds::{apply_final_validated, apply_interim_validated};
use terminal::{apply_overrun_validated, replay_final_observation_validated};

verus! {
pub(in crate::transition) fn observe(
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
    super::super::validation::validate(ledger)?;
    observe_validated(ledger, observation)
}

pub(in crate::transition) fn observe_validated(
    ledger: &mut BudgetLedger,
    observation: UsageObservation,
) -> (result: Result<BudgetReceipt, BudgetError>)
    requires crate::model::ledger_well_formed(old(ledger)),
    ensures
        match result {
            Ok(receipt) => crate::reachability::candidate_step(
                old(ledger),
                crate::BudgetCommand::ObserveUsage(observation),
                final(ledger),
                receipt,
            ),
            Err(error) => crate::reachability::rejection_cause(
                old(ledger),
                crate::BudgetCommand::ObserveUsage(observation),
                error,
            ),
        },
{
    let ghost before = *ledger;
    let reservation_id = observation.verified_reservation_id();
    let reservation_index = match require_reservation(ledger, reservation_id) {
        Ok(index) => index,
        Err(error) => {
            assert(crate::reachability::rejection_cause(
                ledger,
                crate::BudgetCommand::ObserveUsage(observation),
                error,
            ));
            return Err(error);
        }
    };
    let binding = require_binding(
        &ledger.reservations[reservation_index],
        observation.verified_action_id(),
        observation.verified_action_digest(),
    );
    if let Err(error) = binding {
        assert(crate::reachability::reservation_at_guard(
            ledger,
            observation.spec_reservation_id(),
            reservation_index as int,
        ));
        assert(!crate::reachability::observation_binding_guard(
            ledger.reservations[reservation_index as int],
            observation,
        ));
        assert(crate::reachability::rejection_cause(
            ledger,
            crate::BudgetCommand::ObserveUsage(observation),
            error,
        ));
        return Err(error);
    }
    match ledger.reservations[reservation_index].phase {
        ReservationPhase::Active => {}
        _ => {
            return replay_final_observation_validated(
                ledger,
                reservation_index,
                observation,
            );
        }
    }
    proof {
        assert(crate::invariant::ledger_structure_holds(&before));
        assert(crate::invariant::reservation_entry_valid(
            &before,
            reservation_index as int,
        ));
        assert(crate::invariant::record_phase_valid(
            before.reservations[reservation_index as int],
        ));
    }

    let high_water = ledger.reservations[reservation_index].observed;
    let cumulative = observation.verified_cumulative();
    let evidence = observation.verified_evidence_digest();
    let finality = observation.verified_finality();
    let is_interim = match finality {
        UsageFinality::Interim => true,
        UsageFinality::Final => false,
    };
    if !high_water.fits_within(cumulative) {
        let error = BudgetError::reservation(
            BudgetErrorKind::NonmonotonicObservation,
            reservation_id,
        );
        assert(crate::reachability::rejection_cause(
            &before,
            crate::BudgetCommand::ObserveUsage(observation),
            error,
        ));
        return Err(error);
    }
    if high_water.equals(cumulative)
        && ledger.reservations[reservation_index].observation_evidence.is_some()
        && !crate::identity_model::optional_digest_equal(
            ledger.reservations[reservation_index].observation_evidence,
            Some(evidence),
        )
    {
        let error = BudgetError::reservation(
            BudgetErrorKind::BindingMismatch,
            reservation_id,
        );
        assert(crate::reachability::rejection_cause(
            &before,
            crate::BudgetCommand::ObserveUsage(observation),
            error,
        ));
        return Err(error);
    }
    if high_water.equals(cumulative)
        && crate::identity_model::optional_digest_equal(
            ledger.reservations[reservation_index].observation_evidence,
            Some(evidence),
        )
        && is_interim
    {
        let budget_id = ledger.reservations[reservation_index].request.budget_id();
        let replay_receipt = BudgetReceipt::new(
            BudgetOperation::ObserveUsage,
            BudgetReceiptKind::Idempotent,
            budget_id,
            Some(reservation_id),
            BudgetAmounts::zero(),
            BudgetAmounts::zero(),
            Some(cumulative),
            Some(evidence),
        );
        proof {
            crate::reachability::ledger_exact_reflexive(ledger);
            assert(crate::reachability::reservation_bound_to_budget(
                &before,
                observation.spec_reservation_id(),
                budget_id,
            ));
            assert(crate::reachability::observation_receipt_exact(
                replay_receipt,
                observation,
                budget_id,
            ));
            assert(crate::reachability::observation_binding_guard(
                before.reservations[reservation_index as int],
                observation,
            ));
            crate::reachability::observation_guard_from_runtime(
                &before,
                observation,
                BudgetReceiptKind::Idempotent,
                reservation_index as int,
            );
            assert(replay_receipt.spec_kind() == BudgetReceiptKind::Idempotent);
            crate::reachability::observation_refines(
                &before,
                ledger,
                observation,
                replay_receipt,
                budget_id,
            );
        }
        return Ok(replay_receipt);
    }
    let ceiling = ledger.reservations[reservation_index].request.reserve();
    if !cumulative.fits_within(ceiling) {
        proof {
            assert(crate::reachability::observation_binding_guard(
                before.reservations[reservation_index as int],
                observation,
            ));
            crate::reachability::observation_guard_from_runtime(
                &before,
                observation,
                BudgetReceiptKind::OverrunFaulted,
                reservation_index as int,
            );
        }
        return apply_overrun_validated(ledger, reservation_index, observation);
    }

    proof {
        assert(crate::reachability::observation_binding_guard(
            before.reservations[reservation_index as int],
            observation,
        ));
        crate::reachability::observation_guard_from_runtime(
            &before,
            observation,
            BudgetReceiptKind::Applied,
            reservation_index as int,
        );
    }

    match finality {
        UsageFinality::Final => apply_final_validated(ledger, reservation_index, observation),
        UsageFinality::Interim => {
            apply_interim_validated(ledger, reservation_index, observation)
        }
    }
}

} // verus!
