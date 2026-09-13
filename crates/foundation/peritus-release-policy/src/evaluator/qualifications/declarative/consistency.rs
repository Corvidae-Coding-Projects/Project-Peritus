//! Report and verdict consistency lemmas for qualification inputs.

use super::{
    first_report_through, first_verdict_through, reduced_observation,
    reduced_reports_match_first_through,
};
use crate::{QualificationObservation, QualificationSlice, ReleaseCandidate};
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

proof fn first_values_some_iff_reduced_exists(
    values: Seq<QualificationObservation>,
    slice: QualificationSlice,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    end: nat,
)
    requires end <= values.len(),
    ensures
        first_report_through(values, slice, candidate, evaluated_at, end).is_some()
            == (exists |index: int| 0 <= index < end
                && #[trigger] reduced_observation(
                    values[index], slice, candidate, evaluated_at,
                )),
        first_verdict_through(values, slice, candidate, evaluated_at, end).is_some()
            == (exists |index: int| 0 <= index < end
                && #[trigger] reduced_observation(
                    values[index], slice, candidate, evaluated_at,
                )),
    decreases end,
{
    if end > 0 {
        let prior_end = (end - 1) as nat;
        first_values_some_iff_reduced_exists(
            values, slice, candidate, evaluated_at, prior_end,
        );
        let reduced = reduced_observation(
            values[prior_end as int], slice, candidate, evaluated_at,
        );
        assert((exists |index: int| 0 <= index < end
            && #[trigger] reduced_observation(
                values[index], slice, candidate, evaluated_at,
            )) == ((exists |index: int| 0 <= index < prior_end
                && #[trigger] reduced_observation(
                    values[index], slice, candidate, evaluated_at,
                )) || reduced)) by {
            if (exists |index: int| 0 <= index < end
                && #[trigger] reduced_observation(
                    values[index], slice, candidate, evaluated_at,
                )) && !(exists |index: int| 0 <= index < prior_end
                    && #[trigger] reduced_observation(
                        values[index], slice, candidate, evaluated_at,
                    )) {
                let witness = choose |index: int| 0 <= index < end
                    && #[trigger] reduced_observation(
                        values[index], slice, candidate, evaluated_at,
                    );
                assert(witness == prior_end);
            } else if reduced {
                assert(exists |index: int| index == prior_end
                    && 0 <= index < end
                    && #[trigger] reduced_observation(
                        values[index], slice, candidate, evaluated_at,
                    ));
            }
        };
        reveal(first_report_through);
        reveal(first_verdict_through);
    }
}

pub(super) proof fn reduced_reports_match_first_step(
    values: Seq<QualificationObservation>,
    slice: QualificationSlice,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    end: nat,
)
    requires end < values.len(),
    ensures {
        let observation = values[end as int];
        let reduced = reduced_observation(observation, slice, candidate, evaluated_at);
        let first_report = first_report_through(values, slice, candidate, evaluated_at, end);
        let first_verdict = first_verdict_through(values, slice, candidate, evaluated_at, end);
        reduced_reports_match_first_through(
            values, slice, candidate, evaluated_at, (end + 1) as int,
        ) == (reduced_reports_match_first_through(
            values, slice, candidate, evaluated_at, end as int,
        ) && (!reduced || match (first_report, first_verdict) {
            (Some(report), Some(verdict)) => crate::candidate::digest_matches(
                report, observation.spec_report_digest(),
            ) && verdict == observation.spec_verdict(),
            (None, None) => true,
            _ => false,
        }))
    },
{
    let observation = values[end as int];
    let reduced = reduced_observation(observation, slice, candidate, evaluated_at);
    let first_report = first_report_through(values, slice, candidate, evaluated_at, end);
    let first_verdict = first_verdict_through(values, slice, candidate, evaluated_at, end);
    first_values_some_iff_reduced_exists(values, slice, candidate, evaluated_at, end);
    reveal(first_report_through);
    reveal(first_verdict_through);
    reveal(reduced_reports_match_first_through);
    if reduced {
        match (first_report, first_verdict) {
            (Some(report), Some(verdict)) => {
                if reduced_reports_match_first_through(
                    values, slice, candidate, evaluated_at, (end + 1) as int,
                ) {
                    assert(reduced_reports_match_first_through(
                        values, slice, candidate, evaluated_at, end as int,
                    ));
                } else if reduced_reports_match_first_through(
                    values, slice, candidate, evaluated_at, end as int,
                ) && crate::candidate::digest_matches(
                    report, observation.spec_report_digest(),
                ) && verdict == observation.spec_verdict() {
                    assert forall |index: int| 0 <= index < end + 1
                        && #[trigger] reduced_observation(
                            values[index], slice, candidate, evaluated_at,
                        ) implies crate::candidate::digest_matches(
                            report, values[index].spec_report_digest(),
                        ) && verdict == values[index].spec_verdict() by {
                        if index < end {
                            assert(crate::candidate::digest_matches(
                                report, values[index].spec_report_digest(),
                            ) && verdict == values[index].spec_verdict());
                        } else {
                            assert(index == end);
                        }
                    }
                }
            }
            (None, None) => {
                assert(!(exists |index: int| 0 <= index < end
                    && #[trigger] reduced_observation(
                        values[index], slice, candidate, evaluated_at,
                    )));
                assert forall |index: int| 0 <= index < end + 1
                    && #[trigger] reduced_observation(
                        values[index], slice, candidate, evaluated_at,
                    ) implies crate::candidate::digest_matches(
                        observation.spec_report_digest(),
                        values[index].spec_report_digest(),
                    ) && observation.spec_verdict() == values[index].spec_verdict() by {
                    if index < end {
                        assert(false);
                    } else {
                        assert(index == end);
                        digest_matches_itself(observation.spec_report_digest());
                    }
                }
            }
            _ => { assert(false); }
        }
    } else if reduced_reports_match_first_through(
        values, slice, candidate, evaluated_at, end as int,
    ) {
        match (first_report, first_verdict) {
            (Some(report), Some(verdict)) => {
                assert forall |index: int| 0 <= index < end + 1
                    && #[trigger] reduced_observation(
                        values[index], slice, candidate, evaluated_at,
                    ) implies crate::candidate::digest_matches(
                        report, values[index].spec_report_digest(),
                    ) && verdict == values[index].spec_verdict() by {
                    assert(index < end);
                    assert(crate::candidate::digest_matches(
                        report, values[index].spec_report_digest(),
                    ) && verdict == values[index].spec_verdict());
                }
            }
            (None, None) => {}
            _ => { assert(false); }
        }
    }
}

} // verus!
