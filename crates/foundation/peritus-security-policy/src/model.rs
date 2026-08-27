//! Mathematical H0 readiness aggregation.

use vstd::prelude::*;

verus! {

/// Complete conjunction required for an H0-ready result.
pub open spec fn security_ready(
    candidate_bound: bool,
    requirements_complete: bool,
    criteria_complete: bool,
    inventories_complete: bool,
    independent_review_complete: bool,
    blockers_clear: bool,
    evidence_complete: bool,
) -> bool {
    candidate_bound
        && requirements_complete
        && criteria_complete
        && inventories_complete
        && independent_review_complete
        && blockers_clear
        && evidence_complete
}

} // verus!
