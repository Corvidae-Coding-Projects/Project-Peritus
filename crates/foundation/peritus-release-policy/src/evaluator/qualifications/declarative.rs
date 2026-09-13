//! Quantified input characterization of one H0-H3 qualification slice.

mod consistency;

use crate::{
    QualificationObservation, QualificationSlice, QualificationVerdict, ReleaseCandidate,
    ReleaseEvidence,
};
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

pub open spec fn reduced_observation(
    observation: QualificationObservation,
    slice: QualificationSlice,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    observation.spec_slice() == slice
        && observation.spec_binding().spec_is_current_for(candidate, evaluated_at)
        && observation.spec_reviewed()
}

pub open spec fn ready_report_exists_through(
    values: Seq<QualificationObservation>,
    slice: QualificationSlice,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    end: int,
) -> bool {
    exists |index: int| 0 <= index < end
        && #[trigger] values[index].spec_contributes_to(slice, candidate, evaluated_at)
}

pub open spec fn matching_reports_admitted_through(
    values: Seq<QualificationObservation>,
    slice: QualificationSlice,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    end: int,
) -> bool {
    forall |index: int| 0 <= index < end
        && #[trigger] values[index].spec_slice() == slice ==>
            values[index].spec_contributes_to(slice, candidate, evaluated_at)
}

pub open spec fn first_report_through(
    values: Seq<QualificationObservation>,
    slice: QualificationSlice,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    end: nat,
) -> Option<Sha256Digest>
    decreases end,
{
    if end == 0 {
        None
    } else {
        let prior = first_report_through(
            values, slice, candidate, evaluated_at, (end - 1) as nat,
        );
        if prior.is_some() {
            prior
        } else if reduced_observation(
            values[(end - 1) as int], slice, candidate, evaluated_at,
        ) {
            Some(values[(end - 1) as int].spec_report_digest())
        } else {
            None
        }
    }
}

pub open spec fn first_verdict_through(
    values: Seq<QualificationObservation>,
    slice: QualificationSlice,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    end: nat,
) -> Option<QualificationVerdict>
    decreases end,
{
    if end == 0 {
        None
    } else {
        let prior = first_verdict_through(
            values, slice, candidate, evaluated_at, (end - 1) as nat,
        );
        if prior.is_some() {
            prior
        } else if reduced_observation(
            values[(end - 1) as int], slice, candidate, evaluated_at,
        ) {
            Some(values[(end - 1) as int].spec_verdict())
        } else {
            None
        }
    }
}

pub open spec fn reduced_reports_match_first_through(
    values: Seq<QualificationObservation>,
    slice: QualificationSlice,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    end: int,
) -> bool {
    match (
        first_report_through(values, slice, candidate, evaluated_at, end as nat),
        first_verdict_through(values, slice, candidate, evaluated_at, end as nat),
    ) {
        (Some(first_report), Some(first_verdict)) => forall |index: int| 0 <= index < end
            && #[trigger] reduced_observation(values[index], slice, candidate, evaluated_at) ==>
                crate::candidate::digest_matches(
                    first_report, values[index].spec_report_digest(),
                ) && first_verdict == values[index].spec_verdict(),
        (None, None) => true,
        _ => false,
    }
}

/// Direct supplied-report condition for one canonical qualification slice.
pub open spec fn qualification_inputs_ready(
    values: Seq<QualificationObservation>,
    slice: QualificationSlice,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    ready_report_exists_through(values, slice, candidate, evaluated_at, values.len() as int)
        && matching_reports_admitted_through(
            values, slice, candidate, evaluated_at, values.len() as int,
        )
        && reduced_reports_match_first_through(
            values, slice, candidate, evaluated_at, values.len() as int,
        )
}

proof fn characterization_through(
    values: Seq<QualificationObservation>,
    slice: QualificationSlice,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    end: nat,
)
    requires end <= values.len(),
    ensures {
        let state = super::model::through(values, slice, candidate, evaluated_at, end);
        &&& (state.ready_count > 0) == ready_report_exists_through(
            values, slice, candidate, evaluated_at, end as int,
        )
        &&& (state.mismatched_count == 0
                && state.stale_count == 0
                && state.unreviewed_count == 0
                && state.not_ready_count == 0)
            == matching_reports_admitted_through(
                values, slice, candidate, evaluated_at, end as int,
            )
        &&& state.first_report == first_report_through(
            values, slice, candidate, evaluated_at, end,
        )
        &&& state.first_verdict == first_verdict_through(
            values, slice, candidate, evaluated_at, end,
        )
        &&& (!state.conflicting) == reduced_reports_match_first_through(
            values, slice, candidate, evaluated_at, end as int,
        )
    },
    decreases end,
{
    if end > 0 {
        let prior_end = (end - 1) as nat;
        characterization_through(values, slice, candidate, evaluated_at, prior_end);
        consistency::reduced_reports_match_first_step(
            values, slice, candidate, evaluated_at, prior_end,
        );
        let observation = values[prior_end as int];
        let contributes = observation.spec_contributes_to(slice, candidate, evaluated_at);
        reveal(super::model::through);
        reveal(super::model::step);
        reveal(first_report_through);
        reveal(first_verdict_through);
        reveal(reduced_observation);
        reveal(QualificationObservation::spec_contributes_to);
        reveal(crate::EvidenceBinding::spec_is_current_for);
        reveal(crate::EvidenceBinding::spec_is_mismatched);
        reveal(crate::EvidenceBinding::spec_is_stale_at);

        assert(ready_report_exists_through(
            values, slice, candidate, evaluated_at, end as int,
        ) == (ready_report_exists_through(
            values, slice, candidate, evaluated_at, prior_end as int,
        ) || contributes)) by {
            if ready_report_exists_through(
                values, slice, candidate, evaluated_at, end as int,
            ) && !ready_report_exists_through(
                values, slice, candidate, evaluated_at, prior_end as int,
            ) {
                let witness = choose |index: int| 0 <= index < end
                    && #[trigger] values[index]
                        .spec_contributes_to(slice, candidate, evaluated_at);
                assert(witness == prior_end);
            } else if contributes {
                assert(exists |index: int| index == prior_end
                    && 0 <= index < end
                    && #[trigger] values[index]
                        .spec_contributes_to(slice, candidate, evaluated_at));
            }
        };
        assert(matching_reports_admitted_through(
            values, slice, candidate, evaluated_at, end as int,
        ) == (matching_reports_admitted_through(
            values, slice, candidate, evaluated_at, prior_end as int,
        ) && (observation.spec_slice() != slice || contributes))) by {
            if matching_reports_admitted_through(
                values, slice, candidate, evaluated_at, end as int,
            ) {
                assert(matching_reports_admitted_through(
                    values, slice, candidate, evaluated_at, prior_end as int,
                ));
            } else if matching_reports_admitted_through(
                values, slice, candidate, evaluated_at, prior_end as int,
            ) && (observation.spec_slice() != slice || contributes) {
                assert forall |index: int| 0 <= index < end
                    && #[trigger] values[index].spec_slice() == slice implies
                        values[index].spec_contributes_to(slice, candidate, evaluated_at) by {
                    if index < prior_end {
                        assert(values[index].spec_contributes_to(
                            slice, candidate, evaluated_at,
                        ));
                    } else {
                        assert(index == prior_end);
                    }
                }
            }
        };
        reveal(reduced_reports_match_first_through);
    }
}

pub(super) proof fn reduction_matches_inputs(
    evidence: &ReleaseEvidence,
    slice: QualificationSlice,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
)
    ensures
        super::model::qualification_satisfied(
            evidence.spec_qualifications(), slice, candidate, evaluated_at,
        ) == qualification_inputs_ready(
            evidence.spec_qualifications(), slice, candidate, evaluated_at,
        ),
{
    characterization_through(
        evidence.spec_qualifications(),
        slice,
        candidate,
        evaluated_at,
        evidence.spec_qualifications().len() as nat,
    );
    reveal(super::model::qualification_satisfied);
    reveal(super::model::state_satisfied);
    reveal(qualification_inputs_ready);
}

} // verus!
