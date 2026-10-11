//! SEC-INV-001 executable exact-candidate checks.

use crate::{
    IndependentSecurityReview, IntegratedCandidate, ObservationClass, SecurityEvidence,
    UnmetSecurityCondition,
};

fn requirements_current(evidence: &SecurityEvidence, candidate: IntegratedCandidate) -> bool {
    let values = evidence.requirements();
    let mut index = 0;
    while index < values.len() {
        if !crate::binding::candidate_matches(values[index].candidate(), candidate) {
            return false;
        }
        index += 1;
    }
    true
}
fn criteria_current(evidence: &SecurityEvidence, candidate: IntegratedCandidate) -> bool {
    let values = evidence.criteria();
    let mut index = 0;
    while index < values.len() {
        if !crate::binding::candidate_matches(values[index].candidate(), candidate) {
            return false;
        }
        index += 1;
    }
    true
}
fn inventories_current(evidence: &SecurityEvidence, candidate: IntegratedCandidate) -> bool {
    let values = evidence.inventories();
    let mut index = 0;
    while index < values.len() {
        if !crate::binding::candidate_matches(values[index].candidate(), candidate) {
            return false;
        }
        index += 1;
    }
    true
}
fn artifacts_current(evidence: &SecurityEvidence, candidate: IntegratedCandidate) -> bool {
    let values = evidence.artifacts();
    let mut index = 0;
    while index < values.len() {
        if !crate::binding::candidate_matches(values[index].candidate(), candidate) {
            return false;
        }
        index += 1;
    }
    true
}
fn findings_current(review: &IndependentSecurityReview, candidate: IntegratedCandidate) -> bool {
    let values = review.findings();
    let mut index = 0;
    while index < values.len() {
        if !crate::binding::candidate_matches(values[index].candidate(), candidate) {
            return false;
        }
        index += 1;
    }
    true
}
fn report_flat_mismatches(
    candidate: IntegratedCandidate,
    evidence: &SecurityEvidence,
    unmet: &mut Vec<UnmetSecurityCondition>,
) {
    let mut index = 0;
    while index < evidence.requirements().len() {
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
    while index < evidence.criteria().len() {
        if !crate::binding::candidate_matches(evidence.criteria()[index].candidate(), candidate) {
            unmet.push(UnmetSecurityCondition::CandidateMismatch {
                class: ObservationClass::Criterion,
                index,
            });
        }
        index += 1;
    }
    index = 0;
    while index < evidence.inventories().len() {
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
    while index < evidence.artifacts().len() {
        if !crate::binding::candidate_matches(evidence.artifacts()[index].candidate(), candidate) {
            unmet.push(UnmetSecurityCondition::CandidateMismatch {
                class: ObservationClass::Artifact,
                index,
            });
        }
        index += 1;
    }
}
fn report_review_mismatches(
    candidate: IntegratedCandidate,
    evidence: &SecurityEvidence,
    unmet: &mut Vec<UnmetSecurityCondition>,
) {
    if let Some(review) = evidence.review() {
        if !crate::binding::candidate_matches(review.candidate(), candidate) {
            unmet.push(UnmetSecurityCondition::CandidateMismatch {
                class: ObservationClass::ExternalReview,
                index: 0,
            });
        }
        let mut finding_index = 0;
        while finding_index < review.findings().len() {
            let finding_current = crate::binding::candidate_matches(
                review.findings()[finding_index].candidate(),
                candidate,
            );
            if !finding_current {
                unmet.push(UnmetSecurityCondition::CandidateMismatch {
                    class: ObservationClass::Finding,
                    index: finding_index,
                });
            }
            finding_index += 1;
        }
    }
}
#[allow(
    clippy::option_if_let_else,
    reason = "the explicit Option match is supported by Verus while map_or and is_none_or are not"
)]
pub(super) fn evaluate(
    candidate: IntegratedCandidate,
    evidence: &SecurityEvidence,
    unmet: &mut Vec<UnmetSecurityCondition>,
) -> bool {
    report_flat_mismatches(candidate, evidence, unmet);
    report_review_mismatches(candidate, evidence, unmet);
    requirements_current(evidence, candidate)
        && criteria_current(evidence, candidate)
        && inventories_current(evidence, candidate)
        && artifacts_current(evidence, candidate)
        && match evidence.review() {
            Some(review) => {
                crate::binding::candidate_matches(review.candidate(), candidate)
                    && findings_current(review, candidate)
            }
            None => true,
        }
}
