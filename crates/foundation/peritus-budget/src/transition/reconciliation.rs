//! Reservation activation, observation, and terminal reconciliation.

use super::accounting::{bound_receipt, require_binding, require_reservation};
use crate::{
    Activation, BudgetError, BudgetErrorKind, BudgetLedger, BudgetOperation, BudgetReceipt,
    BudgetReceiptKind, ReservationPhase,
};
use vstd::prelude::*;

mod finalization;
mod observation;

pub(super) use finalization::{cancel_held, finalize_ambiguous, settle_exact};
pub(super) use observation::{observe, observe_validated};

verus! {

pub(super) fn activate(
    ledger: &mut BudgetLedger,
    activation: Activation,
) -> (result: Result<BudgetReceipt, BudgetError>)
    requires crate::model::ledger_well_formed(old(ledger)),
    ensures
        match result {
            Ok(receipt) => crate::reachability::candidate_step(
                old(ledger),
                crate::BudgetCommand::Activate(activation),
                final(ledger),
                receipt,
            ),
            Err(error) => crate::reachability::rejection_cause(
                old(ledger),
                crate::BudgetCommand::Activate(activation),
                error,
            ),
        },
{
    let ghost before = *ledger;
    let reservation_index = match require_reservation(
        ledger,
        activation.verified_reservation_id(),
    ) {
        Ok(index) => index,
        Err(error) => {
            assert(crate::reachability::rejection_cause(
                ledger,
                crate::BudgetCommand::Activate(activation),
                error,
            ));
            return Err(error);
        }
    };
    let binding = require_binding(
        &ledger.reservations[reservation_index],
        activation.verified_action_id(),
        activation.verified_action_digest(),
    );
    if let Err(error) = binding {
        assert(crate::reachability::reservation_at_guard(
            ledger,
            activation.spec_reservation_id(),
            reservation_index as int,
        ));
        assert(!crate::reachability::activation_binding_guard(
            ledger.reservations[reservation_index as int],
            activation,
        ));
        assert(crate::reachability::rejection_cause(
            ledger,
            crate::BudgetCommand::Activate(activation),
            error,
        ));
        return Err(error);
    }
    let budget_id = ledger.reservations[reservation_index].request.budget_id();
    match ledger.reservations[reservation_index].phase {
        ReservationPhase::Held => {
            ledger.reservations[reservation_index].phase = ReservationPhase::Active;
            ledger.reservations[reservation_index].activation_evidence =
                Some(activation.verified_evidence_digest());
            let activation_receipt = bound_receipt(
                BudgetOperation::Activate,
                BudgetReceiptKind::Applied,
                budget_id,
                activation.verified_reservation_id(),
                activation.verified_evidence_digest(),
            );
            proof {
                assert(crate::reachability::reservation_bound_to_budget(
                    &before,
                    activation.spec_reservation_id(),
                    budget_id,
                ));
                assert(ledger.reservations@ == before.reservations@.update(
                    reservation_index as int,
                    ledger.reservations[reservation_index as int],
                ));
                crate::reachability::activation_effect_from_update(
                    &before,
                    ledger,
                    activation.spec_reservation_id(),
                    activation.spec_evidence_digest(),
                    reservation_index as int,
                );
                assert(crate::reachability::activation_exact(&before, ledger, activation));
                assert(crate::reachability::activation_receipt_exact(
                    activation_receipt,
                    activation,
                    budget_id,
                ));
                crate::reachability::activation_guard_from_runtime(
                    &before,
                    activation,
                    activation_receipt.spec_kind(),
                    reservation_index as int,
                );
                crate::reachability::activation_refines(
                    &before,
                    ledger,
                    activation,
                    activation_receipt,
                    budget_id,
                );
            }
            Ok(activation_receipt)
        }
        ReservationPhase::Active
            if crate::identity_model::optional_digest_equal(
                ledger.reservations[reservation_index].activation_evidence,
                Some(activation.verified_evidence_digest()),
            ) =>
        {
            let replay_receipt = bound_receipt(
                BudgetOperation::Activate,
                BudgetReceiptKind::Idempotent,
                budget_id,
                activation.verified_reservation_id(),
                activation.verified_evidence_digest(),
            );
            proof {
                crate::reachability::ledger_exact_reflexive(ledger);
                assert(crate::reachability::ledgers_exactly_equal(&before, ledger));
                assert(crate::reachability::reservation_bound_to_budget(
                    &before,
                    activation.spec_reservation_id(),
                    budget_id,
                ));
                assert(crate::reachability::activation_receipt_exact(
                    replay_receipt,
                    activation,
                    budget_id,
                ));
                crate::reachability::activation_guard_from_runtime(
                    &before,
                    activation,
                    replay_receipt.spec_kind(),
                    reservation_index as int,
                );
                crate::reachability::activation_refines(
                    &before,
                    ledger,
                    activation,
                    replay_receipt,
                    budget_id,
                );
            }
            Ok(replay_receipt)
        }
        ReservationPhase::Active => {
            assert(crate::reachability::reservation_at_guard(
                &before,
                activation.spec_reservation_id(),
                reservation_index as int,
            ));
            assert(crate::reachability::activation_binding_guard(
                before.reservations[reservation_index as int],
                activation,
            ));
            assert(!crate::invariant::optional_digests_equal(
                before.reservations[reservation_index as int].activation_evidence,
                Some(activation.spec_evidence_digest()),
            ));
            let error = BudgetError::reservation(
                BudgetErrorKind::BindingMismatch,
                activation.verified_reservation_id(),
            );
            assert(crate::reachability::rejection_cause(
                &before,
                crate::BudgetCommand::Activate(activation),
                error,
            ));
            Err(error)
        }
        _ => {
            assert(crate::reachability::reservation_at_guard(
                &before,
                activation.spec_reservation_id(),
                reservation_index as int,
            ));
            assert(crate::reachability::activation_binding_guard(
                before.reservations[reservation_index as int],
                activation,
            ));
            let error = BudgetError::reservation(
                BudgetErrorKind::InvalidReservationPhase,
                activation.verified_reservation_id(),
            );
            assert(crate::reachability::rejection_cause(
                &before,
                crate::BudgetCommand::Activate(activation),
                error,
            ));
            Err(error)
        }
    }
}

} // verus!
