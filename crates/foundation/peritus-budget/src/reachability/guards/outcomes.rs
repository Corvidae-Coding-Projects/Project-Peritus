//! Observation, finalization, cancellation, and account-lifecycle admissibility.

#[cfg(verus_only)]
use crate::{
    BudgetAccountPhase, BudgetLedger, BudgetReceiptKind, ReservationPhase, UsageFinality,
};
#[cfg(verus_only)]
use peritus_types::BudgetId;
use vstd::prelude::*;

mod lifecycle;

#[cfg(verus_only)]
pub(crate) use lifecycle::lifecycle_guard_from_runtime;

verus! {

pub(crate) open spec fn observation_guard(
    ledger: &BudgetLedger,
    observation: crate::UsageObservation,
    kind: BudgetReceiptKind,
) -> bool {
    exists |index: int| #![auto]
        super::reservation_at(ledger, observation.spec_reservation_id(), index)
            && super::observation_binding(ledger.reservations[index], observation)
            && match ledger.reservations[index].phase {
                ReservationPhase::Active => match kind {
                    BudgetReceiptKind::Idempotent => {
                        ledger.reservations[index].observed.spec_equal(
                            observation.spec_cumulative(),
                        ) && crate::invariant::optional_digests_equal(
                            ledger.reservations[index].observation_evidence,
                            Some(observation.spec_evidence_digest()),
                        ) && observation.spec_finality() == UsageFinality::Interim
                    }
                    BudgetReceiptKind::Applied => {
                        ledger.reservations[index].observed.spec_le(
                            observation.spec_cumulative(),
                        ) && observation.spec_cumulative().spec_le(
                            ledger.reservations[index].request.spec_reserve(),
                        ) && (!ledger.reservations[index].observed.spec_equal(
                            observation.spec_cumulative(),
                        ) || ledger.reservations[index].observation_evidence.is_none()
                            || crate::invariant::optional_digests_equal(
                                ledger.reservations[index].observation_evidence,
                                Some(observation.spec_evidence_digest()),
                            )
                        ) && !(ledger.reservations[index].observed.spec_equal(
                            observation.spec_cumulative(),
                        ) && crate::invariant::optional_digests_equal(
                            ledger.reservations[index].observation_evidence,
                            Some(observation.spec_evidence_digest()),
                        ) && observation.spec_finality() == UsageFinality::Interim)
                    }
                    BudgetReceiptKind::OverrunFaulted => {
                        ledger.reservations[index].observed.spec_le(
                            observation.spec_cumulative(),
                        ) && !observation.spec_cumulative().spec_le(
                            ledger.reservations[index].request.spec_reserve(),
                        )
                    }
                },
                ReservationPhase::SettledFinal => {
                    kind == BudgetReceiptKind::Idempotent
                        && terminal_observation_matches(ledger.reservations[index], observation)
                }
                ReservationPhase::OverrunFaulted => {
                    kind == BudgetReceiptKind::OverrunFaulted
                        && terminal_observation_matches(ledger.reservations[index], observation)
                }
                _ => false,
            }
}

pub(crate) open spec fn terminal_observation_matches(
    record: crate::state::ReservationRecord,
    observation: crate::UsageObservation,
) -> bool {
    record.finality == Some(observation.spec_finality())
        && crate::invariant::optional_digests_equal(
            record.final_evidence,
            Some(observation.spec_evidence_digest()),
        )
        && crate::invariant::optional_amounts_equal(
            record.final_reported,
            Some(observation.spec_cumulative()),
        )
}

pub(crate) open spec fn full_finalization_guard(
    ledger: &BudgetLedger,
    reference: crate::ReservationReference,
    final_phase: ReservationPhase,
    kind: BudgetReceiptKind,
) -> bool {
    exists |index: int| #![auto]
        super::reservation_at(ledger, reference.spec_reservation_id(), index)
            && super::reference_binding(ledger.reservations[index], reference)
            && match kind {
                BudgetReceiptKind::Applied => {
                    ledger.reservations[index].phase == ReservationPhase::Active
                }
                BudgetReceiptKind::Idempotent => {
                    ledger.reservations[index].phase == final_phase
                        && crate::invariant::optional_digests_equal(
                            ledger.reservations[index].final_evidence,
                            Some(reference.spec_evidence_digest()),
                        )
                }
                BudgetReceiptKind::OverrunFaulted => false,
            }
}

pub(crate) open spec fn cancellation_guard(
    ledger: &BudgetLedger,
    reference: crate::ReservationReference,
    kind: BudgetReceiptKind,
) -> bool {
    exists |index: int| #![auto]
        super::reservation_at(ledger, reference.spec_reservation_id(), index)
            && super::reference_binding(ledger.reservations[index], reference)
            && match kind {
                BudgetReceiptKind::Applied => {
                    ledger.reservations[index].phase == ReservationPhase::Held
                }
                BudgetReceiptKind::Idempotent => {
                    ledger.reservations[index].phase == ReservationPhase::CancelledHeld
                        && crate::invariant::optional_digests_equal(
                            ledger.reservations[index].final_evidence,
                            Some(reference.spec_evidence_digest()),
                        )
                }
                BudgetReceiptKind::OverrunFaulted => false,
            }
}

pub(crate) open spec fn lifecycle_guard(
    ledger: &BudgetLedger,
    budget_id: BudgetId,
    kind: BudgetReceiptKind,
    close: bool,
) -> bool {
    exists |index: int| #![auto]
        super::account_at(ledger, budget_id, index)
            && if close {
                match kind {
                    BudgetReceiptKind::Idempotent => {
                        ledger.accounts[index].phase == BudgetAccountPhase::Closed
                    }
                    BudgetReceiptKind::Applied => {
                        (ledger.accounts[index].phase == BudgetAccountPhase::Draining
                            || ledger.accounts[index].phase == BudgetAccountPhase::Faulted)
                            && crate::invariant::budget_has_no_live_work(ledger, budget_id)
                    }
                    BudgetReceiptKind::OverrunFaulted => false,
                }
            } else {
                match kind {
                    BudgetReceiptKind::Applied => {
                        ledger.accounts[index].phase == BudgetAccountPhase::Open
                    }
                    BudgetReceiptKind::Idempotent => {
                        ledger.accounts[index].phase != BudgetAccountPhase::Open
                    }
                    BudgetReceiptKind::OverrunFaulted => false,
                }
            }
}

pub(crate) proof fn observation_guard_from_runtime(
    ledger: &BudgetLedger,
    observation: crate::UsageObservation,
    kind: BudgetReceiptKind,
    index: int,
)
    requires
        crate::model::ledger_well_formed(ledger),
        super::reservation_at(ledger, observation.spec_reservation_id(), index),
        super::observation_binding(ledger.reservations[index], observation),
        match ledger.reservations[index].phase {
            ReservationPhase::Active => match kind {
                BudgetReceiptKind::Idempotent => {
                    ledger.reservations[index].observed.spec_equal(
                        observation.spec_cumulative(),
                    ) && crate::invariant::optional_digests_equal(
                        ledger.reservations[index].observation_evidence,
                        Some(observation.spec_evidence_digest()),
                    ) && observation.spec_finality() == UsageFinality::Interim
                }
                BudgetReceiptKind::Applied => {
                    ledger.reservations[index].observed.spec_le(
                        observation.spec_cumulative(),
                    ) && observation.spec_cumulative().spec_le(
                        ledger.reservations[index].request.spec_reserve(),
                    ) && (!ledger.reservations[index].observed.spec_equal(
                        observation.spec_cumulative(),
                    ) || ledger.reservations[index].observation_evidence.is_none()
                        || crate::invariant::optional_digests_equal(
                            ledger.reservations[index].observation_evidence,
                            Some(observation.spec_evidence_digest()),
                        )
                    ) && !(ledger.reservations[index].observed.spec_equal(
                        observation.spec_cumulative(),
                    ) && crate::invariant::optional_digests_equal(
                        ledger.reservations[index].observation_evidence,
                        Some(observation.spec_evidence_digest()),
                    ) && observation.spec_finality() == UsageFinality::Interim)
                }
                BudgetReceiptKind::OverrunFaulted => {
                    ledger.reservations[index].observed.spec_le(
                        observation.spec_cumulative(),
                    ) && !observation.spec_cumulative().spec_le(
                        ledger.reservations[index].request.spec_reserve(),
                    )
                }
            },
            ReservationPhase::SettledFinal => {
                kind == BudgetReceiptKind::Idempotent
                    && terminal_observation_matches(ledger.reservations[index], observation)
            }
            ReservationPhase::OverrunFaulted => {
                kind == BudgetReceiptKind::OverrunFaulted
                    && terminal_observation_matches(ledger.reservations[index], observation)
            }
            _ => false,
        },
    ensures super::accepted_command_guard(
        ledger,
        crate::BudgetCommand::ObserveUsage(observation),
        kind,
    ),
{
    assert(exists |witness: int| #![auto]
        super::reservation_at(ledger, observation.spec_reservation_id(), witness)
            && super::observation_binding(ledger.reservations[witness], observation)
            && match ledger.reservations[witness].phase {
                ReservationPhase::Active => match kind {
                    BudgetReceiptKind::Idempotent => {
                        ledger.reservations[witness].observed.spec_equal(
                            observation.spec_cumulative(),
                        ) && crate::invariant::optional_digests_equal(
                            ledger.reservations[witness].observation_evidence,
                            Some(observation.spec_evidence_digest()),
                        ) && observation.spec_finality() == UsageFinality::Interim
                    }
                    BudgetReceiptKind::Applied => {
                        ledger.reservations[witness].observed.spec_le(
                            observation.spec_cumulative(),
                        ) && observation.spec_cumulative().spec_le(
                            ledger.reservations[witness].request.spec_reserve(),
                        ) && (!ledger.reservations[witness].observed.spec_equal(
                            observation.spec_cumulative(),
                        ) || ledger.reservations[witness].observation_evidence.is_none()
                            || crate::invariant::optional_digests_equal(
                                ledger.reservations[witness].observation_evidence,
                                Some(observation.spec_evidence_digest()),
                            )
                        ) && !(ledger.reservations[witness].observed.spec_equal(
                            observation.spec_cumulative(),
                        ) && crate::invariant::optional_digests_equal(
                            ledger.reservations[witness].observation_evidence,
                            Some(observation.spec_evidence_digest()),
                        ) && observation.spec_finality() == UsageFinality::Interim)
                    }
                    BudgetReceiptKind::OverrunFaulted => {
                        ledger.reservations[witness].observed.spec_le(
                            observation.spec_cumulative(),
                        ) && !observation.spec_cumulative().spec_le(
                            ledger.reservations[witness].request.spec_reserve(),
                        )
                    }
                },
                ReservationPhase::SettledFinal => {
                    kind == BudgetReceiptKind::Idempotent
                        && terminal_observation_matches(ledger.reservations[witness], observation)
                }
                ReservationPhase::OverrunFaulted => {
                    kind == BudgetReceiptKind::OverrunFaulted
                        && terminal_observation_matches(ledger.reservations[witness], observation)
                }
                _ => false,
            });
}

pub(crate) proof fn finalization_guard_from_runtime(
    ledger: &BudgetLedger,
    reference: crate::ReservationReference,
    final_phase: ReservationPhase,
    kind: BudgetReceiptKind,
    index: int,
)
    requires
        crate::model::ledger_well_formed(ledger),
        super::reservation_at(ledger, reference.spec_reservation_id(), index),
        super::reference_binding(ledger.reservations[index], reference),
        match kind {
            BudgetReceiptKind::Applied => {
                ledger.reservations[index].phase == ReservationPhase::Active
            }
            BudgetReceiptKind::Idempotent => {
                ledger.reservations[index].phase == final_phase
                    && crate::invariant::optional_digests_equal(
                        ledger.reservations[index].final_evidence,
                        Some(reference.spec_evidence_digest()),
                    )
            }
            BudgetReceiptKind::OverrunFaulted => false,
        },
    ensures full_finalization_guard(ledger, reference, final_phase, kind),
{
    assert(exists |witness: int| #![auto]
        super::reservation_at(ledger, reference.spec_reservation_id(), witness)
            && super::reference_binding(ledger.reservations[witness], reference)
            && match kind {
                BudgetReceiptKind::Applied => {
                    ledger.reservations[witness].phase == ReservationPhase::Active
                }
                BudgetReceiptKind::Idempotent => {
                    ledger.reservations[witness].phase == final_phase
                        && crate::invariant::optional_digests_equal(
                            ledger.reservations[witness].final_evidence,
                            Some(reference.spec_evidence_digest()),
                        )
                }
                BudgetReceiptKind::OverrunFaulted => false,
            });
}

pub(crate) proof fn cancellation_guard_from_runtime(
    ledger: &BudgetLedger,
    reference: crate::ReservationReference,
    kind: BudgetReceiptKind,
    index: int,
)
    requires
        crate::model::ledger_well_formed(ledger),
        super::reservation_at(ledger, reference.spec_reservation_id(), index),
        super::reference_binding(ledger.reservations[index], reference),
        match kind {
            BudgetReceiptKind::Applied => {
                ledger.reservations[index].phase == ReservationPhase::Held
            }
            BudgetReceiptKind::Idempotent => {
                ledger.reservations[index].phase == ReservationPhase::CancelledHeld
                    && crate::invariant::optional_digests_equal(
                        ledger.reservations[index].final_evidence,
                        Some(reference.spec_evidence_digest()),
                    )
            }
            BudgetReceiptKind::OverrunFaulted => false,
        },
    ensures super::accepted_command_guard(
        ledger,
        crate::BudgetCommand::CancelHeld(reference),
        kind,
    ),
{
    assert(exists |witness: int| #![auto]
        super::reservation_at(ledger, reference.spec_reservation_id(), witness)
            && super::reference_binding(ledger.reservations[witness], reference)
            && match kind {
                BudgetReceiptKind::Applied => {
                    ledger.reservations[witness].phase == ReservationPhase::Held
                }
                BudgetReceiptKind::Idempotent => {
                    ledger.reservations[witness].phase == ReservationPhase::CancelledHeld
                        && crate::invariant::optional_digests_equal(
                            ledger.reservations[witness].final_evidence,
                            Some(reference.spec_evidence_digest()),
                        )
                }
                BudgetReceiptKind::OverrunFaulted => false,
            });
}

} // verus!
