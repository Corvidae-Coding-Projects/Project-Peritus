//! Digest-consistency lemmas for the quantified artifact characterization.

use super::{
    contributor_exists_through, contributors_match_first_through,
    first_contributor_digest_through,
};
use crate::{EvidenceObservation, EvidenceRequirement, ReleaseCandidate};
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

proof fn digest_matches_itself(digest: Sha256Digest)
    ensures crate::candidate::digest_matches(digest, digest),
{
    proof fn bytes_match_themselves(bytes: [u8; 32], index: nat)
        requires index <= 32,
        ensures crate::candidate::equality::same_bytes_32_from(bytes, bytes, index),
        decreases 32 - index,
    {
        if index < 32 {
            bytes_match_themselves(bytes, index + 1);
            reveal(crate::candidate::equality::same_bytes_32_from);
        }
    }
    bytes_match_themselves(digest.spec_bytes(), 0);
    reveal(crate::candidate::digest_matches);
    reveal(crate::candidate::equality::same_bytes_32);
}

pub(super) proof fn first_digest_some_iff_contributor_exists(
    values: Seq<EvidenceObservation>,
    requirement: EvidenceRequirement,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    end: nat,
)
    requires end <= values.len(),
    ensures
        first_contributor_digest_through(
            values, requirement, candidate, evaluated_at, end,
        ).is_some() == contributor_exists_through(
            values, requirement, candidate, evaluated_at, end as int,
        ),
    decreases end,
{
    if end > 0 {
        let prior_end = (end - 1) as nat;
        first_digest_some_iff_contributor_exists(
            values, requirement, candidate, evaluated_at, prior_end,
        );
        let contributes = values[prior_end as int]
            .spec_contributes_to(requirement, candidate, evaluated_at);
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
        reveal(first_contributor_digest_through);
    }
}

pub(super) proof fn contributors_match_first_step(
    values: Seq<EvidenceObservation>,
    requirement: EvidenceRequirement,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    end: nat,
)
    requires end < values.len(),
    ensures {
        let observation = values[end as int];
        let contributes = observation.spec_contributes_to(
            requirement, candidate, evaluated_at,
        );
        let prior_first = first_contributor_digest_through(
            values, requirement, candidate, evaluated_at, end,
        );
        contributors_match_first_through(
            values, requirement, candidate, evaluated_at, (end + 1) as int,
        ) == (contributors_match_first_through(
            values, requirement, candidate, evaluated_at, end as int,
        ) && (!contributes || match prior_first {
            Some(first) => crate::candidate::digest_matches(
                first, observation.spec_artifact_digest(),
            ),
            None => true,
        }))
    },
{
    let observation = values[end as int];
    let contributes = observation.spec_contributes_to(requirement, candidate, evaluated_at);
    let prior_first = first_contributor_digest_through(
        values, requirement, candidate, evaluated_at, end,
    );
    first_digest_some_iff_contributor_exists(
        values, requirement, candidate, evaluated_at, end,
    );
    reveal(first_contributor_digest_through);
    reveal(contributors_match_first_through);
    if contributes {
        match prior_first {
            Some(first) => {
                if contributors_match_first_through(
                    values, requirement, candidate, evaluated_at, (end + 1) as int,
                ) {
                    assert(contributors_match_first_through(
                        values, requirement, candidate, evaluated_at, end as int,
                    ));
                    assert(crate::candidate::digest_matches(
                        first, observation.spec_artifact_digest(),
                    ));
                } else if contributors_match_first_through(
                    values, requirement, candidate, evaluated_at, end as int,
                ) && crate::candidate::digest_matches(
                    first, observation.spec_artifact_digest(),
                ) {
                    assert forall |index: int| 0 <= index < end + 1
                        && #[trigger] values[index]
                            .spec_contributes_to(requirement, candidate, evaluated_at) implies
                                crate::candidate::digest_matches(
                                    first, values[index].spec_artifact_digest(),
                                ) by {
                        if index < end {
                            assert(crate::candidate::digest_matches(
                                first, values[index].spec_artifact_digest(),
                            ));
                        } else {
                            assert(index == end);
                        }
                    }
                }
            }
            None => {
                assert(!contributor_exists_through(
                    values, requirement, candidate, evaluated_at, end as int,
                ));
                assert forall |index: int| 0 <= index < end implies
                    !#[trigger] values[index]
                        .spec_contributes_to(requirement, candidate, evaluated_at) by {
                    if values[index]
                        .spec_contributes_to(requirement, candidate, evaluated_at)
                    {
                        assert(contributor_exists_through(
                            values, requirement, candidate, evaluated_at, end as int,
                        ));
                    }
                };
                assert forall |index: int| 0 <= index < end + 1
                    && #[trigger] values[index]
                        .spec_contributes_to(requirement, candidate, evaluated_at) implies
                            crate::candidate::digest_matches(
                                observation.spec_artifact_digest(),
                                values[index].spec_artifact_digest(),
                            ) by {
                    assert(index == end);
                    digest_matches_itself(observation.spec_artifact_digest());
                }
            }
        }
    } else {
        if contributors_match_first_through(
            values, requirement, candidate, evaluated_at, end as int,
        ) {
            match prior_first {
                Some(first) => {
                    assert forall |index: int| 0 <= index < end + 1
                        && #[trigger] values[index]
                            .spec_contributes_to(requirement, candidate, evaluated_at) implies
                                crate::candidate::digest_matches(
                                    first, values[index].spec_artifact_digest(),
                                ) by {
                        assert(index < end);
                        assert(crate::candidate::digest_matches(
                            first, values[index].spec_artifact_digest(),
                        ));
                    }
                }
                None => {}
            }
        }
    }
}

} // verus!
