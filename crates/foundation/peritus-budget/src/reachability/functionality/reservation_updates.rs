//! Functionality of single-record reservation effects.

#[cfg(verus_only)]
use crate::{BudgetLedger, ReservationPhase, UsageFinality};
#[cfg(verus_only)]
use peritus_types::{BudgetReservationId, Sha256Digest};
use vstd::prelude::*;

verus! {

proof fn updated_sequences_equal(
    before: Seq<crate::state::ReservationRecord>,
    left_after: Seq<crate::state::ReservationRecord>,
    right_after: Seq<crate::state::ReservationRecord>,
    left_updated: crate::state::ReservationRecord,
    right_updated: crate::state::ReservationRecord,
    index: int,
)
    requires
        0 <= index < before.len(),
        left_after == before.update(index, left_updated),
        right_after == before.update(index, right_updated),
        super::super::reservations::record_exactly_equal(
            left_updated, right_updated,
        ),
    ensures
        left_after.len() == right_after.len(),
        forall |query: int| #![auto]
            0 <= query < left_after.len()
                ==> super::super::reservations::record_exactly_equal(
                    left_after[query], right_after[query],
                ),
{
    assert forall |query: int| #![auto]
        0 <= query < left_after.len()
            implies super::super::reservations::record_exactly_equal(
                left_after[query], right_after[query],
            ) by {
    }
}

pub(super) proof fn observation_effects_equal(
    before: &BudgetLedger,
    left_after: &BudgetLedger,
    right_after: &BudgetLedger,
    reservation_id: BudgetReservationId,
    observed: crate::BudgetAmounts,
    evidence: Sha256Digest,
    phase: ReservationPhase,
    final_reported: Option<crate::BudgetAmounts>,
    finality: Option<UsageFinality>,
)
    requires
        crate::model::ledger_well_formed(before),
        super::super::reservations::observation_effect(
            before, left_after, reservation_id, observed, evidence,
            phase, final_reported, finality,
        ),
        super::super::reservations::observation_effect(
            before, right_after, reservation_id, observed, evidence,
            phase, final_reported, finality,
        ),
    ensures
        left_after.reservations@.len() == right_after.reservations@.len(),
        forall |index: int| #![auto]
            0 <= index < left_after.reservations@.len()
                ==> super::super::reservations::record_exactly_equal(
                    left_after.reservations[index], right_after.reservations[index],
                ),
{
    reveal(super::super::reservations::observation_effect);
    reveal(super::super::reservations::unchanged_except);
    let left_index = choose |index: int| #![auto]
        0 <= index < before.reservations@.len()
            && crate::identity_model::reservation_ids_equal(
                before.reservations[index].request.spec_reservation_id(), reservation_id,
            )
            && left_after.reservations@
                == before.reservations@.update(index, left_after.reservations[index]);
    let right_index = choose |index: int| #![auto]
        0 <= index < before.reservations@.len()
            && crate::identity_model::reservation_ids_equal(
                before.reservations[index].request.spec_reservation_id(), reservation_id,
            )
            && right_after.reservations@
                == before.reservations@.update(index, right_after.reservations[index]);
    crate::invariant::matching_reservations_are_unique(
        before, left_index, right_index,
    );
    assert(left_index == right_index);
    let left_effect = choose |index: int| #![auto]
        0 <= index < before.reservations@.len()
            && crate::identity_model::reservation_ids_equal(
                before.reservations[index].request.spec_reservation_id(), reservation_id,
            ) && {
                let prior = before.reservations[index];
                let next = left_after.reservations[index];
                crate::refinement_model::requests_equal(prior.request, next.request)
                    && next.observed.spec_equal(observed)
                    && next.phase == phase
                    && crate::invariant::optional_digests_equal(
                        prior.activation_evidence, next.activation_evidence,
                    )
                    && crate::invariant::optional_digests_equal(
                        next.observation_evidence, Some(evidence),
                    )
                    && crate::invariant::optional_digests_equal(
                        next.final_evidence,
                        if finality.is_some() { Some(evidence) } else { None },
                    )
                    && crate::invariant::optional_amounts_equal(
                        next.final_reported, final_reported,
                    )
                    && next.finality == finality
            };
    let right_effect = choose |index: int| #![auto]
        0 <= index < before.reservations@.len()
            && crate::identity_model::reservation_ids_equal(
                before.reservations[index].request.spec_reservation_id(), reservation_id,
            ) && {
                let prior = before.reservations[index];
                let next = right_after.reservations[index];
                crate::refinement_model::requests_equal(prior.request, next.request)
                    && next.observed.spec_equal(observed)
                    && next.phase == phase
                    && crate::invariant::optional_digests_equal(
                        prior.activation_evidence, next.activation_evidence,
                    )
                    && crate::invariant::optional_digests_equal(
                        next.observation_evidence, Some(evidence),
                    )
                    && crate::invariant::optional_digests_equal(
                        next.final_evidence,
                        if finality.is_some() { Some(evidence) } else { None },
                    )
                    && crate::invariant::optional_amounts_equal(
                        next.final_reported, final_reported,
                    )
                    && next.finality == finality
            };
    crate::invariant::matching_reservations_are_unique(before, left_index, left_effect);
    crate::invariant::matching_reservations_are_unique(before, left_index, right_effect);
    assert(left_effect == left_index);
    assert(right_effect == left_index);
    assert(super::super::reservations::record_exactly_equal(
        left_after.reservations[left_index],
        right_after.reservations[left_index],
    ));
    updated_sequences_equal(
        before.reservations@,
        left_after.reservations@,
        right_after.reservations@,
        left_after.reservations[left_index],
        right_after.reservations[left_index],
        left_index,
    );
}

pub(super) proof fn finalization_effects_equal(
    before: &BudgetLedger,
    left_after: &BudgetLedger,
    right_after: &BudgetLedger,
    reservation_id: BudgetReservationId,
    evidence: Sha256Digest,
    phase: ReservationPhase,
)
    requires
        crate::model::ledger_well_formed(before),
        super::super::reservations::full_finalization_effect(
            before, left_after, reservation_id, evidence, phase,
        ),
        super::super::reservations::full_finalization_effect(
            before, right_after, reservation_id, evidence, phase,
        ),
    ensures
        left_after.reservations@.len() == right_after.reservations@.len(),
        forall |index: int| #![auto]
            0 <= index < left_after.reservations@.len()
                ==> super::super::reservations::record_exactly_equal(
                    left_after.reservations[index], right_after.reservations[index],
                ),
{
    reveal(super::super::reservations::full_finalization_effect);
    reveal(super::super::reservations::unchanged_except);
    let left_index = choose |index: int| #![auto]
        0 <= index < before.reservations@.len()
            && crate::identity_model::reservation_ids_equal(
                before.reservations[index].request.spec_reservation_id(), reservation_id,
            )
            && left_after.reservations@
                == before.reservations@.update(index, left_after.reservations[index]);
    let right_index = choose |index: int| #![auto]
        0 <= index < before.reservations@.len()
            && crate::identity_model::reservation_ids_equal(
                before.reservations[index].request.spec_reservation_id(), reservation_id,
            )
            && right_after.reservations@
                == before.reservations@.update(index, right_after.reservations[index]);
    crate::invariant::matching_reservations_are_unique(before, left_index, right_index);
    assert(left_index == right_index);
    let left_effect = choose |index: int| #![auto]
        0 <= index < before.reservations@.len()
            && crate::identity_model::reservation_ids_equal(
                before.reservations[index].request.spec_reservation_id(), reservation_id,
            ) && {
                let prior = before.reservations[index];
                let next = left_after.reservations[index];
                crate::refinement_model::requests_equal(prior.request, next.request)
                    && next.observed.spec_equal(prior.request.spec_reserve())
                    && next.phase == phase
                    && crate::invariant::optional_digests_equal(
                        prior.activation_evidence, next.activation_evidence,
                    )
                    && crate::invariant::optional_digests_equal(
                        prior.observation_evidence, next.observation_evidence,
                    )
                    && crate::invariant::optional_digests_equal(
                        next.final_evidence, Some(evidence),
                    )
                    && crate::invariant::optional_amounts_equal(
                        prior.final_reported, next.final_reported,
                    )
                    && prior.finality == next.finality
            };
    let right_effect = choose |index: int| #![auto]
        0 <= index < before.reservations@.len()
            && crate::identity_model::reservation_ids_equal(
                before.reservations[index].request.spec_reservation_id(), reservation_id,
            ) && {
                let prior = before.reservations[index];
                let next = right_after.reservations[index];
                crate::refinement_model::requests_equal(prior.request, next.request)
                    && next.observed.spec_equal(prior.request.spec_reserve())
                    && next.phase == phase
                    && crate::invariant::optional_digests_equal(
                        prior.activation_evidence, next.activation_evidence,
                    )
                    && crate::invariant::optional_digests_equal(
                        prior.observation_evidence, next.observation_evidence,
                    )
                    && crate::invariant::optional_digests_equal(
                        next.final_evidence, Some(evidence),
                    )
                    && crate::invariant::optional_amounts_equal(
                        prior.final_reported, next.final_reported,
                    )
                    && prior.finality == next.finality
            };
    crate::invariant::matching_reservations_are_unique(before, left_index, left_effect);
    crate::invariant::matching_reservations_are_unique(before, left_index, right_effect);
    assert(left_effect == left_index);
    assert(right_effect == left_index);
    assert(super::super::reservations::record_exactly_equal(
        left_after.reservations[left_index], right_after.reservations[left_index],
    ));
    updated_sequences_equal(
        before.reservations@,
        left_after.reservations@,
        right_after.reservations@,
        left_after.reservations[left_index],
        right_after.reservations[left_index],
        left_index,
    );
}

pub(super) proof fn cancellation_effects_equal(
    before: &BudgetLedger,
    left_after: &BudgetLedger,
    right_after: &BudgetLedger,
    reservation_id: BudgetReservationId,
    evidence: Sha256Digest,
)
    requires
        crate::model::ledger_well_formed(before),
        super::super::reservations::cancellation_effect(
            before, left_after, reservation_id, evidence,
        ),
        super::super::reservations::cancellation_effect(
            before, right_after, reservation_id, evidence,
        ),
    ensures
        left_after.reservations@.len() == right_after.reservations@.len(),
        forall |index: int| #![auto]
            0 <= index < left_after.reservations@.len()
                ==> super::super::reservations::record_exactly_equal(
                    left_after.reservations[index], right_after.reservations[index],
                ),
{
    reveal(super::super::reservations::cancellation_effect);
    reveal(super::super::reservations::unchanged_except);
    let left_index = choose |index: int| #![auto]
        0 <= index < before.reservations@.len()
            && crate::identity_model::reservation_ids_equal(
                before.reservations[index].request.spec_reservation_id(), reservation_id,
            )
            && left_after.reservations@
                == before.reservations@.update(index, left_after.reservations[index]);
    let right_index = choose |index: int| #![auto]
        0 <= index < before.reservations@.len()
            && crate::identity_model::reservation_ids_equal(
                before.reservations[index].request.spec_reservation_id(), reservation_id,
            )
            && right_after.reservations@
                == before.reservations@.update(index, right_after.reservations[index]);
    crate::invariant::matching_reservations_are_unique(before, left_index, right_index);
    assert(left_index == right_index);
    let left_effect = choose |index: int| #![auto]
        0 <= index < before.reservations@.len()
            && crate::identity_model::reservation_ids_equal(
                before.reservations[index].request.spec_reservation_id(), reservation_id,
            ) && {
                let prior = before.reservations[index];
                let next = left_after.reservations[index];
                crate::refinement_model::requests_equal(prior.request, next.request)
                    && prior.observed.spec_equal(next.observed)
                    && next.phase == ReservationPhase::CancelledHeld
                    && crate::invariant::optional_digests_equal(
                        prior.activation_evidence, next.activation_evidence,
                    )
                    && crate::invariant::optional_digests_equal(
                        prior.observation_evidence, next.observation_evidence,
                    )
                    && crate::invariant::optional_digests_equal(
                        next.final_evidence, Some(evidence),
                    )
                    && crate::invariant::optional_amounts_equal(
                        prior.final_reported, next.final_reported,
                    )
                    && prior.finality == next.finality
            };
    let right_effect = choose |index: int| #![auto]
        0 <= index < before.reservations@.len()
            && crate::identity_model::reservation_ids_equal(
                before.reservations[index].request.spec_reservation_id(), reservation_id,
            ) && {
                let prior = before.reservations[index];
                let next = right_after.reservations[index];
                crate::refinement_model::requests_equal(prior.request, next.request)
                    && prior.observed.spec_equal(next.observed)
                    && next.phase == ReservationPhase::CancelledHeld
                    && crate::invariant::optional_digests_equal(
                        prior.activation_evidence, next.activation_evidence,
                    )
                    && crate::invariant::optional_digests_equal(
                        prior.observation_evidence, next.observation_evidence,
                    )
                    && crate::invariant::optional_digests_equal(
                        next.final_evidence, Some(evidence),
                    )
                    && crate::invariant::optional_amounts_equal(
                        prior.final_reported, next.final_reported,
                    )
                    && prior.finality == next.finality
            };
    crate::invariant::matching_reservations_are_unique(before, left_index, left_effect);
    crate::invariant::matching_reservations_are_unique(before, left_index, right_effect);
    assert(left_effect == left_index);
    assert(right_effect == left_index);
    assert(super::super::reservations::record_exactly_equal(
        left_after.reservations[left_index], right_after.reservations[left_index],
    ));
    updated_sequences_equal(
        before.reservations@,
        left_after.reservations@,
        right_after.reservations@,
        left_after.reservations[left_index],
        right_after.reservations[left_index],
        left_index,
    );
}

} // verus!
