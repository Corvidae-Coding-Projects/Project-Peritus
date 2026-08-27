//! SEC-INV-001 executable exact-candidate checks.

use crate::{
    IntegratedCandidate, ObservationClass, SecurityEvidence, UnmetSecurityCondition,
};
use vstd::prelude::*;

verus! {

fn requirements_current(
    evidence: &SecurityEvidence,
    candidate: IntegratedCandidate,
) -> (current: bool)
    ensures current == (forall |index: int| 0 <= index < evidence.spec_requirements().len() ==>
        #[trigger] crate::binding::candidate_fresh(
            evidence.spec_requirements()[index].spec_candidate(), candidate)),
{
    let values = evidence.requirements();
    let mut index = 0;
    while index < values.len()
        invariant
            0 <= index <= values.len(),
            values@ == evidence.spec_requirements(),
            forall |prior: int| 0 <= prior < index ==>
                #[trigger] crate::binding::candidate_fresh(
                    values@[prior].spec_candidate(), candidate),
        decreases values.len() - index,
    {
        if !crate::binding::candidate_matches(values[index].candidate(), candidate) {
            assert(values@ == evidence.spec_requirements());
            assert(!(forall |prior: int| 0 <= prior < evidence.spec_requirements().len() ==>
                #[trigger] crate::binding::candidate_fresh(
                    evidence.spec_requirements()[prior].spec_candidate(), candidate))) by {
                assert(!crate::binding::candidate_fresh(
                    evidence.spec_requirements()[index as int].spec_candidate(), candidate));
            };
            return false;
        }
        index += 1;
    }
    true
}

fn criteria_current(
    evidence: &SecurityEvidence,
    candidate: IntegratedCandidate,
) -> (current: bool)
    ensures current == (forall |index: int| 0 <= index < evidence.spec_criteria().len() ==>
        #[trigger] crate::binding::candidate_fresh(
            evidence.spec_criteria()[index].spec_candidate(), candidate)),
{
    let values = evidence.criteria();
    let mut index = 0;
    while index < values.len()
        invariant
            0 <= index <= values.len(),
            values@ == evidence.spec_criteria(),
            forall |prior: int| 0 <= prior < index ==>
                #[trigger] crate::binding::candidate_fresh(
                    values@[prior].spec_candidate(), candidate),
        decreases values.len() - index,
    {
        if !crate::binding::candidate_matches(values[index].candidate(), candidate) {
            assert(values@ == evidence.spec_criteria());
            assert(!(forall |prior: int| 0 <= prior < evidence.spec_criteria().len() ==>
                #[trigger] crate::binding::candidate_fresh(
                    evidence.spec_criteria()[prior].spec_candidate(), candidate))) by {
                assert(!crate::binding::candidate_fresh(
                    evidence.spec_criteria()[index as int].spec_candidate(), candidate));
            };
            return false;
        }
        index += 1;
    }
    true
}

fn inventories_current(
    evidence: &SecurityEvidence,
    candidate: IntegratedCandidate,
) -> (current: bool)
    ensures current == (forall |index: int| 0 <= index < evidence.spec_inventories().len() ==>
        #[trigger] crate::binding::candidate_fresh(
            evidence.spec_inventories()[index].spec_candidate(), candidate)),
{
    let values = evidence.inventories();
    let mut index = 0;
    while index < values.len()
        invariant
            0 <= index <= values.len(),
            values@ == evidence.spec_inventories(),
            forall |prior: int| 0 <= prior < index ==>
                #[trigger] crate::binding::candidate_fresh(
                    values@[prior].spec_candidate(), candidate),
        decreases values.len() - index,
    {
        if !crate::binding::candidate_matches(values[index].candidate(), candidate) {
            assert(values@ == evidence.spec_inventories());
            assert(!(forall |prior: int| 0 <= prior < evidence.spec_inventories().len() ==>
                #[trigger] crate::binding::candidate_fresh(
                    evidence.spec_inventories()[prior].spec_candidate(), candidate))) by {
                assert(!crate::binding::candidate_fresh(
                    evidence.spec_inventories()[index as int].spec_candidate(), candidate));
            };
            return false;
        }
        index += 1;
    }
    true
}

fn artifacts_current(
    evidence: &SecurityEvidence,
    candidate: IntegratedCandidate,
) -> (current: bool)
    ensures current == (forall |index: int| 0 <= index < evidence.spec_artifacts().len() ==>
        #[trigger] crate::binding::candidate_fresh(
            evidence.spec_artifacts()[index].spec_candidate(), candidate)),
{
    let values = evidence.artifacts();
    let mut index = 0;
    while index < values.len()
        invariant
            0 <= index <= values.len(),
            values@ == evidence.spec_artifacts(),
            forall |prior: int| 0 <= prior < index ==>
                #[trigger] crate::binding::candidate_fresh(
                    values@[prior].spec_candidate(), candidate),
        decreases values.len() - index,
    {
        if !crate::binding::candidate_matches(values[index].candidate(), candidate) {
            assert(values@ == evidence.spec_artifacts());
            assert(!(forall |prior: int| 0 <= prior < evidence.spec_artifacts().len() ==>
                #[trigger] crate::binding::candidate_fresh(
                    evidence.spec_artifacts()[prior].spec_candidate(), candidate))) by {
                assert(!crate::binding::candidate_fresh(
                    evidence.spec_artifacts()[index as int].spec_candidate(), candidate));
            };
            return false;
        }
        index += 1;
    }
    true
}

#[allow(
    clippy::option_if_let_else,
    reason = "the explicit Option match is supported by Verus while map_or and is_none_or are not"
)]
pub(super) fn evaluate(
    candidate: IntegratedCandidate,
    evidence: &SecurityEvidence,
    unmet: &mut Vec<UnmetSecurityCondition>,
) -> (current: bool)
    ensures current == evidence.spec_all_current(candidate),
{
    let mut index = 0;
    while index < evidence.requirements().len()
        invariant 0 <= index <= evidence.spec_requirements().len(),
        decreases evidence.spec_requirements().len() - index,
    {
        if !crate::binding::candidate_matches(evidence.requirements()[index].candidate(), candidate)
        {
            unmet.push(UnmetSecurityCondition::CandidateMismatch {
                class: ObservationClass::Requirement,
                index,
            });
        }
        index += 1;
    }
    index = 0;
    while index < evidence.criteria().len()
        invariant 0 <= index <= evidence.spec_criteria().len(),
        decreases evidence.spec_criteria().len() - index,
    {
        if !crate::binding::candidate_matches(evidence.criteria()[index].candidate(), candidate) {
            unmet.push(UnmetSecurityCondition::CandidateMismatch {
                class: ObservationClass::Criterion,
                index,
            });
        }
        index += 1;
    }
    index = 0;
    while index < evidence.inventories().len()
        invariant 0 <= index <= evidence.spec_inventories().len(),
        decreases evidence.spec_inventories().len() - index,
    {
        if !crate::binding::candidate_matches(evidence.inventories()[index].candidate(), candidate)
        {
            unmet.push(UnmetSecurityCondition::CandidateMismatch {
                class: ObservationClass::Inventory,
                index,
            });
        }
        index += 1;
    }
    index = 0;
    while index < evidence.artifacts().len()
        invariant 0 <= index <= evidence.spec_artifacts().len(),
        decreases evidence.spec_artifacts().len() - index,
    {
        if !crate::binding::candidate_matches(evidence.artifacts()[index].candidate(), candidate) {
            unmet.push(UnmetSecurityCondition::CandidateMismatch {
                class: ObservationClass::Artifact,
                index,
            });
        }
        index += 1;
    }
    if let Some(review) = evidence.review() {
        if !crate::binding::candidate_matches(review.candidate(), candidate) {
            unmet.push(UnmetSecurityCondition::CandidateMismatch {
                class: ObservationClass::ExternalReview,
                index: 0,
            });
        }
        let mut finding_index = 0;
        while finding_index < review.findings().len()
            invariant 0 <= finding_index <= review.spec_findings().len(),
            decreases review.spec_findings().len() - finding_index,
        {
            if !crate::binding::candidate_matches(
                review.findings()[finding_index].candidate(), candidate,
            ) {
                unmet.push(UnmetSecurityCondition::CandidateMismatch {
                    class: ObservationClass::Finding,
                    index: finding_index,
                });
            }
            finding_index += 1;
        }
    }

    requirements_current(evidence, candidate)
        && criteria_current(evidence, candidate)
        && inventories_current(evidence, candidate)
        && artifacts_current(evidence, candidate)
        && match evidence.review() {
            Some(review) => crate::binding::candidate_matches(review.candidate(), candidate),
            None => true,
        }
}

} // verus!
