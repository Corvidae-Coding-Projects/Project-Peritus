//! Required canonical evidence-manifest role checks.

use crate::{
    EvidenceArtifactKind, IntegratedCandidate, SecurityEvidence, UnmetSecurityCondition,
};
use vstd::prelude::*;

verus! {

fn current_artifact(
    evidence: &SecurityEvidence,
    target: EvidenceArtifactKind,
    candidate: IntegratedCandidate,
) -> Option<&crate::ArtifactObservation> {
    let values = evidence.artifacts();
    let mut index = 0;
    while index < values.len()
        invariant 0 <= index <= values.len(),
        decreases values.len() - index,
    {
        if values[index].kind() == target
            && crate::binding::candidate_matches(values[index].candidate(), candidate)
        {
            return Some(&values[index]);
        }
        index += 1;
    }
    None
}

pub(super) fn evaluate(
    candidate: IntegratedCandidate,
    evidence: &SecurityEvidence,
    unmet: &mut Vec<UnmetSecurityCondition>,
) -> bool {
    let mut complete = true;
    let mut index = 0;
    while index < EvidenceArtifactKind::ALL.len()
        invariant 0 <= index <= EvidenceArtifactKind::ALL.len(),
        decreases EvidenceArtifactKind::ALL.len() - index,
    {
        let kind = EvidenceArtifactKind::ALL[index];
        match current_artifact(evidence, kind, candidate) {
            None => {
                complete = false;
                unmet.push(UnmetSecurityCondition::MissingEvidenceArtifact(kind));
            }
            Some(observation) if !crate::binding::digest_present(observation.digest()) => {
                complete = false;
                unmet.push(UnmetSecurityCondition::EmptyEvidenceDigest(kind));
            }
            Some(_) => {}
        }
        index += 1;
    }
    complete
}

} // verus!
