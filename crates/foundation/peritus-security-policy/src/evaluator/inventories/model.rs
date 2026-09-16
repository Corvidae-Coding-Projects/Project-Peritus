//! Actual-input specification for canonical inventory evaluation.

#[cfg(verus_only)]
use crate::{IntegratedCandidate, InventoryKind, InventoryObservation, SecurityEvidence};
use vstd::prelude::*;

verus! {

pub open spec fn observation_matches(
    observation: InventoryObservation,
    target: InventoryKind,
    candidate: IntegratedCandidate,
) -> bool {
    observation.spec_kind() == target
        && crate::binding::candidate_fresh(observation.spec_candidate(), candidate)
}

pub open spec fn first_observation_at(
    values: Seq<InventoryObservation>,
    index: int,
    target: InventoryKind,
    candidate: IntegratedCandidate,
) -> bool {
    0 <= index < values.len()
        && observation_matches(values[index], target, candidate)
        && (forall |prior: int| 0 <= prior < index ==>
            !observation_matches(#[trigger] values[prior], target, candidate))
}

pub open spec fn admitted(
    values: Seq<InventoryObservation>,
    target: InventoryKind,
    candidate: IntegratedCandidate,
) -> bool {
    exists |index: int| {
        let observation = #[trigger] values[index];
        &&& first_observation_at(values, index, target, candidate)
        &&& observation.spec_complete()
        &&& crate::binding::digest_is_present(observation.spec_evidence_digest())
    }
}

pub open spec fn complete_through(
    evidence: &SecurityEvidence,
    candidate: IntegratedCandidate,
    end: int,
) -> bool {
    forall |index: int| 0 <= index < end ==>
        #[trigger] admitted(
            evidence.spec_inventories(),
            InventoryKind::ALL[index],
            candidate,
        )
}

pub open spec fn complete(
    evidence: &SecurityEvidence,
    candidate: IntegratedCandidate,
) -> bool {
    complete_through(evidence, candidate, InventoryKind::ALL.len() as int)
}

pub proof fn first_index_unique(
    values: Seq<InventoryObservation>,
    left: int,
    right: int,
    target: InventoryKind,
    candidate: IntegratedCandidate,
)
    requires
        first_observation_at(values, left, target, candidate),
        first_observation_at(values, right, target, candidate),
    ensures left == right,
{
    if left < right {
        assert(!observation_matches(values[left], target, candidate));
    } else if right < left {
        assert(!observation_matches(values[right], target, candidate));
    }
}

pub proof fn admission_at_first(
    values: Seq<InventoryObservation>,
    index: int,
    target: InventoryKind,
    candidate: IntegratedCandidate,
)
    requires first_observation_at(values, index, target, candidate),
    ensures admitted(values, target, candidate) == (
        values[index].spec_complete()
            && crate::binding::digest_is_present(values[index].spec_evidence_digest())
    ),
{
    if admitted(values, target, candidate) {
        let other = choose |other: int| {
            let observation = #[trigger] values[other];
            &&& first_observation_at(values, other, target, candidate)
            &&& observation.spec_complete()
            &&& crate::binding::digest_is_present(observation.spec_evidence_digest())
        };
        first_index_unique(values, index, other, target, candidate);
    } else if values[index].spec_complete()
        && crate::binding::digest_is_present(values[index].spec_evidence_digest())
    {
        assert(admitted(values, target, candidate)) by {
            assert(exists |witness: int| {
                let observation = #[trigger] values[witness];
                &&& witness == index
                &&& first_observation_at(values, witness, target, candidate)
                &&& observation.spec_complete()
                &&& crate::binding::digest_is_present(observation.spec_evidence_digest())
            });
        }
    }
}

pub proof fn complete_step(
    evidence: &SecurityEvidence,
    candidate: IntegratedCandidate,
    end: int,
)
    requires 0 <= end < InventoryKind::ALL.len(),
    ensures complete_through(evidence, candidate, end + 1) == (
        complete_through(evidence, candidate, end)
            && admitted(
                evidence.spec_inventories(),
                InventoryKind::ALL[end],
                candidate,
            )
    ),
{
    let current = admitted(
        evidence.spec_inventories(),
        InventoryKind::ALL[end],
        candidate,
    );
    if complete_through(evidence, candidate, end + 1) {
        assert(complete_through(evidence, candidate, end));
        assert(current);
    } else if complete_through(evidence, candidate, end) && current {
        assert(complete_through(evidence, candidate, end + 1)) by {
            assert forall |index: int| 0 <= index < end + 1 implies
                #[trigger] admitted(
                    evidence.spec_inventories(),
                    InventoryKind::ALL[index],
                    candidate,
                ) by {
                if index < end {
                    assert(admitted(
                        evidence.spec_inventories(),
                        InventoryKind::ALL[index],
                        candidate,
                    ));
                } else {
                    assert(index == end);
                }
            }
        };
    }
}

} // verus!
