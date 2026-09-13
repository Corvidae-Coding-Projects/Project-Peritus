//! Quantified input characterization of one release-artifact requirement.

use crate::{EvidenceObservation, EvidenceRequirement, ReleaseCandidate};
use peritus_types::Sha256Digest;
use vstd::prelude::*;

mod consistency;

verus! {


pub open spec fn contributor_exists_through(
    values: Seq<EvidenceObservation>,
    requirement: EvidenceRequirement,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    end: int,
) -> bool {
    exists |index: int| 0 <= index < end
        && #[trigger] values[index].spec_contributes_to(requirement, candidate, evaluated_at)
}

pub open spec fn matching_observations_admitted_through(
    values: Seq<EvidenceObservation>,
    requirement: EvidenceRequirement,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    end: int,
) -> bool {
    forall |index: int| 0 <= index < end
        && #[trigger] values[index].spec_requirement() == requirement ==>
            values[index].spec_contributes_to(requirement, candidate, evaluated_at)
}

pub open spec fn first_contributor_digest_through(
    values: Seq<EvidenceObservation>,
    requirement: EvidenceRequirement,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    end: nat,
) -> Option<Sha256Digest>
    decreases end,
{
    if end == 0 {
        None
    } else {
        let prior = first_contributor_digest_through(
            values, requirement, candidate, evaluated_at, (end - 1) as nat,
        );
        if prior.is_some() {
            prior
        } else if values[(end - 1) as int]
            .spec_contributes_to(requirement, candidate, evaluated_at)
        {
            Some(values[(end - 1) as int].spec_artifact_digest())
        } else {
            None
        }
    }
}

pub open spec fn contributors_match_first_through(
    values: Seq<EvidenceObservation>,
    requirement: EvidenceRequirement,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    end: int,
) -> bool {
    match first_contributor_digest_through(
        values, requirement, candidate, evaluated_at, end as nat,
    ) {
        Some(first) => forall |index: int| 0 <= index < end
            && #[trigger] values[index]
                .spec_contributes_to(requirement, candidate, evaluated_at) ==>
                    crate::candidate::digest_matches(
                        first,
                        values[index].spec_artifact_digest(),
                    ),
        None => true,
    }
}

pub open spec fn requirement_inputs_ready_through(
    values: Seq<EvidenceObservation>,
    requirement: EvidenceRequirement,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    end: int,
) -> bool {
    contributor_exists_through(values, requirement, candidate, evaluated_at, end)
        && matching_observations_admitted_through(
            values, requirement, candidate, evaluated_at, end,
        )
        && contributors_match_first_through(
            values, requirement, candidate, evaluated_at, end,
        )
}

pub open spec fn requirement_inputs_ready(
    values: Seq<EvidenceObservation>,
    requirement: EvidenceRequirement,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    requirement_inputs_ready_through(
        values, requirement, candidate, evaluated_at, values.len() as int,
    )
}


proof fn characterization_through(
    values: Seq<EvidenceObservation>,
    requirement: EvidenceRequirement,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    end: nat,
)
    requires end <= values.len(),
    ensures {
        let state = super::model::through(
            values, requirement, candidate, evaluated_at, end,
        );
        &&& (state.contributing_count > 0) == contributor_exists_through(
            values, requirement, candidate, evaluated_at, end as int,
        )
        &&& (state.mismatched_count == 0
                && state.stale_count == 0
                && state.wrong_source_count == 0
                && state.unreviewed_count == 0
                && state.unsigned_count == 0)
            == matching_observations_admitted_through(
                values, requirement, candidate, evaluated_at, end as int,
            )
        &&& state.first_digest == first_contributor_digest_through(
            values, requirement, candidate, evaluated_at, end,
        )
        &&& (!state.conflicting) == contributors_match_first_through(
            values, requirement, candidate, evaluated_at, end as int,
        )
    },
    decreases end,
{
    if end > 0 {
        let prior_end = (end - 1) as nat;
        characterization_through(values, requirement, candidate, evaluated_at, prior_end);
        consistency::first_digest_some_iff_contributor_exists(
            values, requirement, candidate, evaluated_at, prior_end,
        );
        consistency::contributors_match_first_step(
            values, requirement, candidate, evaluated_at, prior_end,
        );
        let observation = values[prior_end as int];
        let prior = super::model::through(
            values, requirement, candidate, evaluated_at, prior_end,
        );
        let contributes = observation.spec_contributes_to(
            requirement, candidate, evaluated_at,
        );
        reveal(super::model::through);
        reveal(super::model::step);
        reveal(first_contributor_digest_through);
        reveal(EvidenceObservation::spec_contributes_to);
        reveal(crate::EvidenceBinding::spec_is_current_for);
        reveal(crate::EvidenceBinding::spec_is_mismatched);
        reveal(crate::EvidenceBinding::spec_is_stale_at);

        assert(contributor_exists_through(
            values, requirement, candidate, evaluated_at, end as int,
        ) == (contributor_exists_through(
            values, requirement, candidate, evaluated_at, prior_end as int,
        ) || contributes)) by {
            if contributor_exists_through(
                values, requirement, candidate, evaluated_at, end as int,
            ) && !contributor_exists_through(
                values, requirement, candidate, evaluated_at, prior_end as int,
            ) {
                let witness = choose |index: int| 0 <= index < end
                    && #[trigger] values[index]
                        .spec_contributes_to(requirement, candidate, evaluated_at);
                assert(witness == prior_end);
            } else if contributes {
                assert(exists |index: int| index == prior_end
                    && 0 <= index < end
                    && #[trigger] values[index]
                        .spec_contributes_to(requirement, candidate, evaluated_at));
            }
        };

        assert(matching_observations_admitted_through(
            values, requirement, candidate, evaluated_at, end as int,
        ) == (matching_observations_admitted_through(
            values, requirement, candidate, evaluated_at, prior_end as int,
        ) && (observation.spec_requirement() != requirement || contributes))) by {
            if matching_observations_admitted_through(
                values, requirement, candidate, evaluated_at, end as int,
            ) {
                assert(matching_observations_admitted_through(
                    values, requirement, candidate, evaluated_at, prior_end as int,
                ));
            } else if matching_observations_admitted_through(
                values, requirement, candidate, evaluated_at, prior_end as int,
            ) && (observation.spec_requirement() != requirement || contributes) {
                assert forall |index: int| 0 <= index < end
                    && #[trigger] values[index].spec_requirement() == requirement implies
                        values[index]
                            .spec_contributes_to(requirement, candidate, evaluated_at) by {
                    if index < prior_end {
                        assert(values[index]
                            .spec_contributes_to(requirement, candidate, evaluated_at));
                    } else {
                        assert(index == prior_end);
                    }
                }
            }
        };

        reveal(contributors_match_first_through);
    }
    let state = super::model::through(values, requirement, candidate, evaluated_at, end);
    assert((state.contributing_count > 0) == contributor_exists_through(
        values, requirement, candidate, evaluated_at, end as int,
    ));
    assert((state.mismatched_count == 0
            && state.stale_count == 0
            && state.wrong_source_count == 0
            && state.unreviewed_count == 0
            && state.unsigned_count == 0)
        == matching_observations_admitted_through(
            values, requirement, candidate, evaluated_at, end as int,
        ));
    assert(state.first_digest == first_contributor_digest_through(
        values, requirement, candidate, evaluated_at, end,
    ));
    assert((!state.conflicting) == contributors_match_first_through(
        values, requirement, candidate, evaluated_at, end as int,
    ));
}

pub(super) proof fn reduction_matches_inputs(
    evidence: &crate::ReleaseEvidence,
    requirement: EvidenceRequirement,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
)
    ensures
        super::model::requirement_satisfied(
            evidence,
            requirement,
            candidate,
            evaluated_at,
        ) == requirement_inputs_ready(
            evidence.spec_observations(), requirement, candidate, evaluated_at,
        ),
{
    characterization_through(
        evidence.spec_observations(),
        requirement,
        candidate,
        evaluated_at,
        evidence.spec_observations().len() as nat,
    );
    reveal(super::model::requirement_satisfied);
    reveal(super::model::state_satisfied);
    reveal(requirement_inputs_ready);
    reveal(requirement_inputs_ready_through);
}

} // verus!
