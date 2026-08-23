//! Exact reservation-vector effects used by accepted command steps.

#[cfg(verus_only)]
use crate::{BudgetAmounts, BudgetLedger, BudgetRequest, ReservationPhase, UsageFinality};
#[cfg(verus_only)]
use peritus_types::{BudgetReservationId, Sha256Digest};
use vstd::prelude::*;

verus! {

pub(crate) open spec fn record_exactly_equal(
    before: crate::state::ReservationRecord,
    after: crate::state::ReservationRecord,
) -> bool {
    crate::refinement_model::requests_equal(before.request, after.request)
        && before.observed.spec_equal(after.observed)
        && before.phase == after.phase
        && crate::invariant::optional_digests_equal(
            before.activation_evidence,
            after.activation_evidence,
        )
        && crate::invariant::optional_digests_equal(
            before.observation_evidence,
            after.observation_evidence,
        )
        && crate::invariant::optional_digests_equal(
            before.final_evidence,
            after.final_evidence,
        )
        && crate::invariant::optional_amounts_equal(
            before.final_reported,
            after.final_reported,
        )
        && before.finality == after.finality
}

pub(crate) open spec fn unchanged_except(
    before: &BudgetLedger,
    after: &BudgetLedger,
    reservation_id: BudgetReservationId,
) -> bool {
    exists |index: int| #![auto]
        0 <= index < before.reservations@.len()
            && crate::identity_model::reservation_ids_equal(
                before.reservations[index].request.spec_reservation_id(),
                reservation_id,
            )
            && after.reservations@
                == before.reservations@.update(index, after.reservations[index])
}

pub(crate) open spec fn begin_record_effect(
    before: &BudgetLedger,
    after: &BudgetLedger,
    request: BudgetRequest,
) -> bool {
    after.reservations@ == before.reservations@.push(
        after.reservations[before.reservations@.len() as int],
    )
        && {
            let record = after.reservations[before.reservations@.len() as int];
            crate::refinement_model::requests_equal(record.request, request)
                && record.observed.spec_is_zero()
                && record.phase == if request.spec_reserve().spec_is_zero() {
                    ReservationPhase::SettledExact
                } else {
                    ReservationPhase::Held
                }
                && record.activation_evidence.is_none()
                && record.observation_evidence.is_none()
                && record.final_evidence.is_none()
                && record.final_reported.is_none()
                && record.finality.is_none()
        }
}

pub(crate) open spec fn activation_effect(
    before: &BudgetLedger,
    after: &BudgetLedger,
    reservation_id: BudgetReservationId,
    evidence: Sha256Digest,
) -> bool {
    unchanged_except(before, after, reservation_id)
        && exists |index: int| #![auto]
            activation_record_effect(before, after, reservation_id, evidence, index)
}

pub(crate) open spec fn activation_record_effect(
    before: &BudgetLedger,
    after: &BudgetLedger,
    reservation_id: BudgetReservationId,
    evidence: Sha256Digest,
    index: int,
) -> bool {
    0 <= index < before.reservations@.len()
        && crate::identity_model::reservation_ids_equal(
            before.reservations[index].request.spec_reservation_id(),
            reservation_id,
        )
        && {
            let prior = before.reservations[index];
            let next = after.reservations[index];
            crate::refinement_model::requests_equal(prior.request, next.request)
                && prior.observed.spec_equal(next.observed)
                && next.phase == ReservationPhase::Active
                && crate::invariant::optional_digests_equal(
                    next.activation_evidence,
                    Some(evidence),
                )
                && crate::invariant::optional_digests_equal(
                    prior.observation_evidence,
                    next.observation_evidence,
                )
                && crate::invariant::optional_digests_equal(
                    prior.final_evidence,
                    next.final_evidence,
                )
                && crate::invariant::optional_amounts_equal(
                    prior.final_reported,
                    next.final_reported,
                )
                && prior.finality == next.finality
        }
}

pub(crate) proof fn activation_effect_parts(
    before: &BudgetLedger,
    after: &BudgetLedger,
    reservation_id: BudgetReservationId,
    evidence: Sha256Digest,
)
    requires activation_effect(before, after, reservation_id, evidence),
    ensures
        unchanged_except(before, after, reservation_id),
        exists |index: int| #![auto]
            activation_record_effect(before, after, reservation_id, evidence, index),
{
}

pub(crate) proof fn activation_effect_from_update(
    before: &BudgetLedger,
    after: &BudgetLedger,
    reservation_id: BudgetReservationId,
    evidence: Sha256Digest,
    index: int,
)
    requires
        activation_record_effect(before, after, reservation_id, evidence, index),
        after.reservations@
            == before.reservations@.update(index, after.reservations[index]),
    ensures activation_effect(before, after, reservation_id, evidence),
{
    assert(unchanged_except(before, after, reservation_id));
    assert(exists |witness: int| #![auto]
        activation_record_effect(before, after, reservation_id, evidence, witness));
}

pub(crate) proof fn unchanged_except_has_witness(
    before: &BudgetLedger,
    after: &BudgetLedger,
    reservation_id: BudgetReservationId,
)
    requires unchanged_except(before, after, reservation_id),
    ensures exists |index: int| #![auto]
        0 <= index < before.reservations@.len()
            && crate::identity_model::reservation_ids_equal(
                before.reservations[index].request.spec_reservation_id(),
                reservation_id,
            )
            && after.reservations@
                == before.reservations@.update(index, after.reservations[index]),
{
}

pub(crate) open spec fn observation_effect(
    before: &BudgetLedger,
    after: &BudgetLedger,
    reservation_id: BudgetReservationId,
    observed: BudgetAmounts,
    evidence: Sha256Digest,
    phase: ReservationPhase,
    final_reported: Option<BudgetAmounts>,
    finality: Option<UsageFinality>,
) -> bool {
    unchanged_except(before, after, reservation_id)
        && exists |index: int| #![auto]
            0 <= index < before.reservations@.len()
                && crate::identity_model::reservation_ids_equal(
                    before.reservations[index].request.spec_reservation_id(),
                    reservation_id,
                )
                && {
                    let prior = before.reservations[index];
                    let next = after.reservations[index];
                    crate::refinement_model::requests_equal(prior.request, next.request)
                        && next.observed.spec_equal(observed)
                        && next.phase == phase
                        && crate::invariant::optional_digests_equal(
                            prior.activation_evidence,
                            next.activation_evidence,
                        )
                        && crate::invariant::optional_digests_equal(
                            next.observation_evidence,
                            Some(evidence),
                        )
                        && crate::invariant::optional_digests_equal(
                            next.final_evidence,
                            if finality.is_some() { Some(evidence) } else { None },
                        )
                        && crate::invariant::optional_amounts_equal(
                            next.final_reported,
                            final_reported,
                        )
                        && next.finality == finality
                }
}

pub(crate) open spec fn full_finalization_effect(
    before: &BudgetLedger,
    after: &BudgetLedger,
    reservation_id: BudgetReservationId,
    evidence: Sha256Digest,
    phase: ReservationPhase,
) -> bool {
    unchanged_except(before, after, reservation_id)
        && exists |index: int| #![auto]
            0 <= index < before.reservations@.len()
                && crate::identity_model::reservation_ids_equal(
                    before.reservations[index].request.spec_reservation_id(),
                    reservation_id,
                )
                && {
                    let prior = before.reservations[index];
                    let next = after.reservations[index];
                    crate::refinement_model::requests_equal(prior.request, next.request)
                        && next.observed.spec_equal(prior.request.spec_reserve())
                        && next.phase == phase
                        && crate::invariant::optional_digests_equal(
                            prior.activation_evidence,
                            next.activation_evidence,
                        )
                        && crate::invariant::optional_digests_equal(
                            prior.observation_evidence,
                            next.observation_evidence,
                        )
                        && crate::invariant::optional_digests_equal(
                            next.final_evidence,
                            Some(evidence),
                        )
                        && crate::invariant::optional_amounts_equal(
                            prior.final_reported,
                            next.final_reported,
                        )
                        && prior.finality == next.finality
                }
}

pub(crate) open spec fn cancellation_effect(
    before: &BudgetLedger,
    after: &BudgetLedger,
    reservation_id: BudgetReservationId,
    evidence: Sha256Digest,
) -> bool {
    unchanged_except(before, after, reservation_id)
        && exists |index: int| #![auto]
            0 <= index < before.reservations@.len()
                && crate::identity_model::reservation_ids_equal(
                    before.reservations[index].request.spec_reservation_id(),
                    reservation_id,
                )
                && {
                    let prior = before.reservations[index];
                    let next = after.reservations[index];
                    crate::refinement_model::requests_equal(prior.request, next.request)
                        && prior.observed.spec_equal(next.observed)
                        && next.phase == ReservationPhase::CancelledHeld
                        && crate::invariant::optional_digests_equal(
                            prior.activation_evidence,
                            next.activation_evidence,
                        )
                        && crate::invariant::optional_digests_equal(
                            prior.observation_evidence,
                            next.observation_evidence,
                        )
                        && crate::invariant::optional_digests_equal(
                            next.final_evidence,
                            Some(evidence),
                        )
                        && crate::invariant::optional_amounts_equal(
                            prior.final_reported,
                            next.final_reported,
                        )
                        && prior.finality == next.finality
                }
}

} // verus!
