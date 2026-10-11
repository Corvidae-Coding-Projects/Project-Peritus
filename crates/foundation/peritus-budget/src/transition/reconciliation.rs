//! Reservation activation, observation, and terminal reconciliation.

use super::accounting::{bound_receipt, require_binding, require_reservation};
use crate::{
    Activation, BudgetError, BudgetErrorKind, BudgetLedger, BudgetOperation, BudgetReceipt,
    BudgetReceiptKind, ReservationPhase,
};

mod finalization;
mod observation;

pub(super) use finalization::{cancel_held, finalize_ambiguous, settle_exact};
pub(super) use observation::{observe, observe_validated};

pub(super) fn activate(
    ledger: &mut BudgetLedger,
    activation: Activation,
) -> Result<BudgetReceipt, BudgetError> {
    let reservation_index = match require_reservation(ledger, activation.verified_reservation_id())
    {
        Ok(index) => index,
        Err(error) => {
            return Err(error);
        }
    };
    let binding = require_binding(
        &ledger.reservations[reservation_index],
        activation.verified_action_id(),
        activation.verified_action_digest(),
    );
    if let Err(error) = binding {
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
            Ok(replay_receipt)
        }
        ReservationPhase::Active => {
            let error = BudgetError::reservation(
                BudgetErrorKind::BindingMismatch,
                activation.verified_reservation_id(),
            );
            Err(error)
        }
        _ => {
            let error = BudgetError::reservation(
                BudgetErrorKind::InvalidReservationPhase,
                activation.verified_reservation_id(),
            );
            Err(error)
        }
    }
}
