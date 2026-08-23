//! Exact replay of terminal observations.

use crate::{
    BudgetAmounts, BudgetError, BudgetErrorKind, BudgetLedger, BudgetOperation, BudgetReceipt,
    BudgetReceiptKind, ReservationPhase, UsageObservation,
};
use vstd::prelude::*;

verus! {

pub(in crate::transition) fn replay_final_observation_validated(
    ledger: &BudgetLedger,
    reservation_index: usize,
    observation: UsageObservation,
) -> (result: Result<BudgetReceipt, BudgetError>)
    requires
        crate::model::ledger_well_formed(ledger),
        (reservation_index as int) < ledger.reservations@.len(),
        crate::identity_model::reservation_ids_equal(
            ledger.reservations[reservation_index as int].request.spec_reservation_id(),
            observation.spec_reservation_id(),
        ),
        crate::reachability::observation_binding_guard(
            ledger.reservations[reservation_index as int],
            observation,
        ),
        ledger.reservations[reservation_index as int].phase != ReservationPhase::Active,
    ensures
        match result {
            Ok(receipt) => crate::reachability::candidate_step(
                ledger,
                crate::BudgetCommand::ObserveUsage(observation),
                ledger,
                receipt,
            ),
            Err(error) => crate::reachability::rejection_cause(
                ledger,
                crate::BudgetCommand::ObserveUsage(observation),
                error,
            ),
        },
{
    let record = &ledger.reservations[reservation_index];
    assert(crate::invariant::ledger_structure_holds(ledger));
    assert(crate::invariant::reservation_entry_valid(
        ledger,
        reservation_index as int,
    ));
    assert(crate::invariant::record_phase_valid(*record));
    let reported_matches = match record.final_reported {
        Some(reported) => reported.equals(observation.verified_cumulative()),
        None => false,
    };
    let finality_matches = match (record.finality, observation.verified_finality()) {
        (Some(crate::UsageFinality::Interim), crate::UsageFinality::Interim)
        | (Some(crate::UsageFinality::Final), crate::UsageFinality::Final) => true,
        _ => false,
    };
    let exact_replay = finality_matches
        && crate::identity_model::optional_digest_equal(
            record.final_evidence,
            Some(observation.verified_evidence_digest()),
        )
        && reported_matches;
    if !exact_replay {
        let error = BudgetError::reservation(
            BudgetErrorKind::InvalidReservationPhase,
            observation.verified_reservation_id(),
        );
        assert(crate::reachability::reservation_at_guard(
            ledger,
            observation.spec_reservation_id(),
            reservation_index as int,
        ));
        assert(!crate::reachability::terminal_observation_guard(*record, observation));
        assert(crate::reachability::rejection_cause(
            ledger,
            crate::BudgetCommand::ObserveUsage(observation),
            error,
        ));
        return Err(error);
    }
    let replay_receipt = match record.phase {
        ReservationPhase::SettledFinal => {
            let receipt = BudgetReceipt::new(
                BudgetOperation::ObserveUsage,
                BudgetReceiptKind::Idempotent,
                record.request.budget_id(),
                Some(observation.verified_reservation_id()),
                BudgetAmounts::zero(),
                BudgetAmounts::zero(),
                Some(observation.verified_cumulative()),
                Some(observation.verified_evidence_digest()),
            );
            proof {
                crate::reachability::observation_guard_from_runtime(
                    ledger,
                    observation,
                    BudgetReceiptKind::Idempotent,
                    reservation_index as int,
                );
            }
            receipt
        }
        ReservationPhase::OverrunFaulted => {
            let receipt = BudgetReceipt::new(
                BudgetOperation::ObserveUsage,
                BudgetReceiptKind::OverrunFaulted,
                record.request.budget_id(),
                Some(observation.verified_reservation_id()),
                BudgetAmounts::zero(),
                BudgetAmounts::zero(),
                Some(observation.verified_cumulative()),
                Some(observation.verified_evidence_digest()),
            );
            proof {
                crate::reachability::observation_guard_from_runtime(
                    ledger,
                    observation,
                    BudgetReceiptKind::OverrunFaulted,
                    reservation_index as int,
                );
            }
            receipt
        }
        _ => {
            assert(false);
            let error = BudgetError::reservation(
                BudgetErrorKind::InvalidReservationPhase,
                observation.verified_reservation_id(),
            );
            return Err(error);
        }
    };
    proof {
        crate::reachability::ledger_exact_reflexive(ledger);
        assert(crate::reachability::reservation_bound_to_budget(
            ledger,
            observation.spec_reservation_id(),
            record.request.spec_budget_id(),
        ));
        assert(crate::reachability::observation_receipt_exact(
            replay_receipt,
            observation,
            record.request.spec_budget_id(),
        ));
        crate::reachability::overrun_terminal_effect_exact(
            ledger,
            observation,
            replay_receipt,
            record.request.spec_budget_id(),
            reservation_index as int,
        );
        assert(exists |index: int| #![auto]
            crate::reachability::observation_overrun_effect(
                ledger,
                observation,
                ledger,
                replay_receipt,
                record.request.spec_budget_id(),
                index,
            ));
        crate::reachability::observation_refines(
            ledger,
            ledger,
            observation,
            replay_receipt,
            record.request.spec_budget_id(),
        );
    }
    Ok(replay_receipt)
}


} // verus!
