//! Branch-ordered typed rejection relation for total reducer outcomes.

#[cfg(verus_only)]
use crate::{
    BudgetAccountPhase, BudgetCommand, BudgetError, BudgetErrorKind, BudgetLedger,
    ReservationPhase,
};
use vstd::prelude::*;

pub(crate) mod allocation;
mod reservation;

verus! {

pub(crate) open spec fn exact_budget_error(
    error: BudgetError,
    kind: BudgetErrorKind,
    budget_id: peritus_types::BudgetId,
) -> bool {
    error.spec_kind() == kind
        && crate::identity_model::parents_equal(error.spec_budget_id(), Some(budget_id))
        && error.spec_reservation_id().is_none()
        && error.spec_limiting_dimensions().spec_is_empty()
        && error.spec_arithmetic().is_none()
}

pub(crate) open spec fn exact_reservation_error(
    error: BudgetError,
    kind: BudgetErrorKind,
    reservation_id: peritus_types::BudgetReservationId,
) -> bool {
    error.spec_kind() == kind
        && error.spec_budget_id().is_none()
        && crate::state::optional_reservation_ids_equal(
            error.spec_reservation_id(),
            Some(reservation_id),
        )
        && error.spec_limiting_dimensions().spec_is_empty()
        && error.spec_arithmetic().is_none()
}

pub(crate) open spec fn exact_arithmetic_error(
    error: BudgetError,
    arithmetic: crate::AmountArithmeticError,
) -> bool {
    error.spec_kind() == BudgetErrorKind::Arithmetic
        && error.spec_budget_id().is_none()
        && error.spec_reservation_id().is_none()
        && error.spec_limiting_dimensions().spec_is_empty()
        && error.spec_arithmetic() == Some(arithmetic)
}

pub(crate) open spec fn exact_insufficient_error(
    error: BudgetError,
    budget_id: peritus_types::BudgetId,
    requested: crate::BudgetAmounts,
    available: crate::BudgetAmounts,
) -> bool {
    error.spec_kind() == BudgetErrorKind::InsufficientBudget
        && budget_error_matches(error, budget_id)
        && error.spec_limiting_dimensions().spec_bits()
            == crate::BudgetAmounts::spec_exceeding_bits(requested, available)
        && error.spec_limiting_dimensions().spec_bits() != 0
        && error.spec_arithmetic().is_none()
}

pub(crate) open spec fn budget_error_matches(
    error: BudgetError,
    budget_id: peritus_types::BudgetId,
) -> bool {
    crate::identity_model::parents_equal(error.spec_budget_id(), Some(budget_id))
        && error.spec_reservation_id().is_none()
}

pub(crate) open spec fn reservation_error_matches(
    error: BudgetError,
    reservation_id: peritus_types::BudgetReservationId,
) -> bool {
    error.spec_budget_id().is_none()
        && crate::state::optional_reservation_ids_equal(
            error.spec_reservation_id(),
            Some(reservation_id),
        )
}

pub(crate) open spec fn infrastructure_error(error: BudgetError) -> bool {
    match error.spec_kind() {
        BudgetErrorKind::Arithmetic => {
            error.spec_budget_id().is_none()
                && error.spec_reservation_id().is_none()
                && error.spec_limiting_dimensions().spec_is_empty()
                && error.spec_arithmetic().is_some()
        }
        BudgetErrorKind::CorruptState => {
            error.spec_budget_id().is_some()
                && error.spec_reservation_id().is_none()
                && error.spec_limiting_dimensions().spec_is_empty()
                && error.spec_arithmetic().is_none()
        }
        _ => false,
    }
}

pub(crate) open spec fn no_account(
    ledger: &BudgetLedger,
    budget_id: peritus_types::BudgetId,
) -> bool {
    forall |index: int| #![auto]
        0 <= index < ledger.accounts@.len()
            ==> !crate::identity_model::budget_ids_equal(ledger.accounts[index].id, budget_id)
}

pub(crate) open spec fn no_reservation(
    ledger: &BudgetLedger,
    reservation_id: peritus_types::BudgetReservationId,
) -> bool {
    forall |index: int| #![auto]
        0 <= index < ledger.reservations@.len()
            ==> !crate::identity_model::reservation_ids_equal(
                ledger.reservations[index].request.spec_reservation_id(),
                reservation_id,
            )
}

pub(crate) open spec fn activation_rejection(
    ledger: &BudgetLedger,
    activation: crate::Activation,
    error: BudgetError,
) -> bool {
    (no_reservation(ledger, activation.spec_reservation_id())
        && exact_reservation_error(
            error,
            BudgetErrorKind::UnknownReservation,
            activation.spec_reservation_id(),
        ))
        || ((exists |index: int| #![auto]
                super::guards::reservation_at(
                    ledger,
                    activation.spec_reservation_id(),
                    index,
                ) && !super::guards::activation_binding(
                    ledger.reservations[index],
                    activation,
                ))
            && exact_reservation_error(
                error,
                BudgetErrorKind::BindingMismatch,
                activation.spec_reservation_id(),
            ))
        || ((exists |index: int| #![auto]
                super::guards::reservation_at(
                    ledger,
                    activation.spec_reservation_id(),
                    index,
                ) && super::guards::activation_binding(
                    ledger.reservations[index],
                    activation,
                ) && ledger.reservations[index].phase == ReservationPhase::Active
                    && !crate::invariant::optional_digests_equal(
                        ledger.reservations[index].activation_evidence,
                        Some(activation.spec_evidence_digest()),
                    ))
            && exact_reservation_error(
                error,
                BudgetErrorKind::BindingMismatch,
                activation.spec_reservation_id(),
            ))
        || ((exists |index: int| #![auto]
                super::guards::reservation_at(
                    ledger,
                    activation.spec_reservation_id(),
                    index,
                ) && super::guards::activation_binding(
                    ledger.reservations[index],
                    activation,
                ) && ledger.reservations[index].phase != ReservationPhase::Held
                    && ledger.reservations[index].phase != ReservationPhase::Active)
            && exact_reservation_error(
                error,
                BudgetErrorKind::InvalidReservationPhase,
                activation.spec_reservation_id(),
            ))
}

pub(crate) open spec fn observation_rejection(
    ledger: &BudgetLedger,
    observation: crate::UsageObservation,
    error: BudgetError,
) -> bool {
    reservation::observation_rejection(ledger, observation, error)
}

pub(crate) open spec fn lifecycle_rejection(
    ledger: &BudgetLedger,
    budget_id: peritus_types::BudgetId,
    error: BudgetError,
    close: bool,
) -> bool {
    (no_account(ledger, budget_id)
        && exact_budget_error(error, BudgetErrorKind::UnknownBudget, budget_id))
        || (close
            && (exists |index: int| #![auto]
                super::guards::account_at(ledger, budget_id, index)
                    && ledger.accounts[index].phase == BudgetAccountPhase::Open)
            && exact_budget_error(error, BudgetErrorKind::InvalidAccountPhase, budget_id))
        || (close
            && !crate::invariant::budget_has_no_live_work(ledger, budget_id)
            && (exists |index: int| #![auto]
                super::guards::account_at(ledger, budget_id, index)
                    && (ledger.accounts[index].phase == BudgetAccountPhase::Draining
                        || ledger.accounts[index].phase == BudgetAccountPhase::Faulted))
            && exact_budget_error(error, BudgetErrorKind::OutstandingWork, budget_id))
}

pub(crate) open spec fn rejection_cause(
    ledger: &BudgetLedger,
    command: BudgetCommand,
    error: BudgetError,
) -> bool {
    crate::model::ledger_well_formed(ledger)
        && match command {
            BudgetCommand::AllocateChild(request) => {
                allocation::allocation_rejection(ledger, request, error)
            }
            BudgetCommand::Begin(request) => allocation::begin_rejection(ledger, request, error),
            BudgetCommand::Activate(activation) => {
                activation_rejection(ledger, activation, error)
            }
            BudgetCommand::ObserveUsage(observation) => {
                observation_rejection(ledger, observation, error)
            }
            BudgetCommand::SettleExact(reference) => reservation::finalization_rejection(
                ledger,
                reference,
                ReservationPhase::SettledExact,
                error,
            ),
            BudgetCommand::CancelHeld(reference) => {
                reservation::cancellation_rejection(ledger, reference, error)
            }
            BudgetCommand::FinalizeAmbiguous(finalization) => {
                reservation::finalization_rejection(
                    ledger,
                    finalization.spec_reference(),
                    ReservationPhase::SettledAmbiguous,
                    error,
                )
            }
            BudgetCommand::Seal(budget_id) => lifecycle_rejection(ledger, budget_id, error, false),
            BudgetCommand::Close(budget_id) => lifecycle_rejection(ledger, budget_id, error, true),
        }
}

} // verus!
