//! Branch-ordered reservation reconciliation rejection predicates.

#[cfg(verus_only)]
use crate::{BudgetError, BudgetErrorKind, BudgetLedger, ReservationPhase};
use vstd::prelude::*;

verus! {

pub(crate) open spec fn observation_rejection(
    ledger: &BudgetLedger,
    observation: crate::UsageObservation,
    error: BudgetError,
) -> bool {
    (super::no_reservation(ledger, observation.spec_reservation_id())
        && super::exact_reservation_error(
            error,
            BudgetErrorKind::UnknownReservation,
            observation.spec_reservation_id(),
        ))
        || ((exists |index: int| #![auto]
            super::super::guards::reservation_at(
                ledger,
                observation.spec_reservation_id(),
                index,
            ) && !super::super::guards::observation_binding(
                ledger.reservations[index],
                observation,
            )) && super::exact_reservation_error(
                error,
                BudgetErrorKind::BindingMismatch,
                observation.spec_reservation_id(),
            ))
        || (exists |index: int| #![auto]
            super::super::guards::reservation_at(
                ledger,
                observation.spec_reservation_id(),
                index,
            ) && super::super::guards::observation_binding(
                ledger.reservations[index],
                observation,
            ) && match ledger.reservations[index].phase {
                ReservationPhase::Active => active_observation_rejection(
                    ledger.reservations[index],
                    observation,
                    error,
                ),
                ReservationPhase::SettledFinal | ReservationPhase::OverrunFaulted => {
                    !super::super::guards::terminal_observation_matches(
                        ledger.reservations[index],
                        observation,
                    ) && super::exact_reservation_error(
                            error,
                            BudgetErrorKind::InvalidReservationPhase,
                            observation.spec_reservation_id(),
                        )
                }
                _ => super::exact_reservation_error(
                    error,
                    BudgetErrorKind::InvalidReservationPhase,
                    observation.spec_reservation_id(),
                ),
            })
}

pub(crate) open spec fn active_observation_rejection(
    record: crate::state::ReservationRecord,
    observation: crate::UsageObservation,
    error: BudgetError,
) -> bool {
    if !record.observed.spec_le(observation.spec_cumulative()) {
        super::exact_reservation_error(
            error,
            BudgetErrorKind::NonmonotonicObservation,
            observation.spec_reservation_id(),
        )
    } else if record.observed.spec_equal(observation.spec_cumulative())
        && record.observation_evidence.is_some()
        && !crate::invariant::optional_digests_equal(
            record.observation_evidence,
            Some(observation.spec_evidence_digest()),
        )
    {
        super::exact_reservation_error(
            error,
            BudgetErrorKind::BindingMismatch,
            observation.spec_reservation_id(),
        )
    } else {
        false
    }
}

pub(crate) open spec fn finalization_rejection(
    ledger: &BudgetLedger,
    reference: crate::ReservationReference,
    final_phase: ReservationPhase,
    error: BudgetError,
) -> bool {
    (super::no_reservation(ledger, reference.spec_reservation_id())
        && super::exact_reservation_error(
            error,
            BudgetErrorKind::UnknownReservation,
            reference.spec_reservation_id(),
        ))
        || ((exists |index: int| #![auto]
            super::super::guards::reservation_at(
                ledger,
                reference.spec_reservation_id(),
                index,
            ) && !super::super::guards::reference_binding(
                ledger.reservations[index], reference,
            )) && super::exact_reservation_error(
                error,
                BudgetErrorKind::BindingMismatch,
                reference.spec_reservation_id(),
            ))
        || ((exists |index: int| #![auto]
            super::super::guards::reservation_at(
                ledger,
                reference.spec_reservation_id(),
                index,
            ) && super::super::guards::reference_binding(ledger.reservations[index], reference)
                && ledger.reservations[index].phase != ReservationPhase::Active
                && !(ledger.reservations[index].phase == final_phase
                    && crate::invariant::optional_digests_equal(
                        ledger.reservations[index].final_evidence,
                        Some(reference.spec_evidence_digest()),
                    ))) && super::exact_reservation_error(
                error,
                BudgetErrorKind::InvalidReservationPhase,
                reference.spec_reservation_id(),
            ))
}

pub(crate) open spec fn cancellation_rejection(
    ledger: &BudgetLedger,
    reference: crate::ReservationReference,
    error: BudgetError,
) -> bool {
    (super::no_reservation(ledger, reference.spec_reservation_id())
        && super::exact_reservation_error(
            error,
            BudgetErrorKind::UnknownReservation,
            reference.spec_reservation_id(),
        ))
        || ((exists |index: int| #![auto]
            super::super::guards::reservation_at(
                ledger,
                reference.spec_reservation_id(),
                index,
            ) && !super::super::guards::reference_binding(
                ledger.reservations[index], reference,
            )) && super::exact_reservation_error(
                error,
                BudgetErrorKind::BindingMismatch,
                reference.spec_reservation_id(),
            ))
        || ((exists |index: int| #![auto]
            super::super::guards::reservation_at(
                ledger,
                reference.spec_reservation_id(),
                index,
            ) && super::super::guards::reference_binding(ledger.reservations[index], reference)
                && ledger.reservations[index].phase != ReservationPhase::Held
                && !(ledger.reservations[index].phase == ReservationPhase::CancelledHeld
                    && crate::invariant::optional_digests_equal(
                        ledger.reservations[index].final_evidence,
                        Some(reference.spec_evidence_digest()),
                    ))) && super::exact_reservation_error(
                error,
                BudgetErrorKind::InvalidReservationPhase,
                reference.spec_reservation_id(),
            ))
}

} // verus!
