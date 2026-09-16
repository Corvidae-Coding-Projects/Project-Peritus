//! Finding and waiver policy reduction.

#[cfg(verus_only)]
mod declarative;
#[cfg(verus_only)]
mod model;
mod lookup;
mod reduction;

pub(super) use reduction::assess;

#[cfg(verus_only)]
use crate::{FindingAssessment, ReleaseCandidate, ReleaseEvidence};
use vstd::prelude::*;

verus! {

/// Exact stored fields produced by the complete supplied finding and waiver reduction.
pub open spec fn assessment_matches_reduction(
    assessment: FindingAssessment,
    evidence: &ReleaseEvidence,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    let state = model::final_state(evidence, candidate, evaluated_at);
    &&& assessment.spec_is_satisfied() == model::state_satisfied(state)
    &&& assessment.spec_stale_count() == state.stale_count
    &&& assessment.spec_mismatched_count() == state.mismatched_count
    &&& assessment.spec_open_count() == state.open_count
    &&& assessment.spec_release_blocking_count() == state.release_blocking_count
    &&& assessment.spec_ignored_count() == state.ignored_count
    &&& assessment.spec_quarantined_count() == state.quarantined_count
    &&& assessment.spec_invalid_waiver_count() == state.invalid_waiver_count
    &&& assessment.spec_conflicting_finding() == state.conflicting_finding
}

pub open spec fn findings_satisfied(
    evidence: &ReleaseEvidence,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    declarative::finding_inputs_ready(evidence, candidate, evaluated_at)
}

} // verus!
