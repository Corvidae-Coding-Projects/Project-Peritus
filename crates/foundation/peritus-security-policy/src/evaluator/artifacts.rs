//! Required canonical evidence-manifest role checks.

mod model;

use crate::{
    EvidenceArtifactKind, IntegratedCandidate, SecurityEvidence, UnmetSecurityCondition,
};
use vstd::prelude::*;

verus! {

pub open spec fn evidence_complete(
    evidence: &SecurityEvidence,
    candidate: IntegratedCandidate,
) -> bool {
    model::complete(evidence, candidate)
}

const fn kinds_equal(left: EvidenceArtifactKind, right: EvidenceArtifactKind) -> (equal: bool)
    ensures equal == (left == right),
{
    matches!((left, right),
        (EvidenceArtifactKind::CampaignPlan, EvidenceArtifactKind::CampaignPlan)
        | (EvidenceArtifactKind::NativeProbeResults, EvidenceArtifactKind::NativeProbeResults)
        | (EvidenceArtifactKind::ResourceAccounting, EvidenceArtifactKind::ResourceAccounting)
        | (EvidenceArtifactKind::CleanupLedger, EvidenceArtifactKind::CleanupLedger)
        | (EvidenceArtifactKind::ThreatControlInventory, EvidenceArtifactKind::ThreatControlInventory)
        | (EvidenceArtifactKind::UnsafeTcbInventory, EvidenceArtifactKind::UnsafeTcbInventory)
        | (EvidenceArtifactKind::ExternalReviewReport, EvidenceArtifactKind::ExternalReviewReport)
        | (EvidenceArtifactKind::FindingRegister, EvidenceArtifactKind::FindingRegister)
        | (EvidenceArtifactKind::SupplyChainAttestation, EvidenceArtifactKind::SupplyChainAttestation)
        | (EvidenceArtifactKind::ReleaseManifest, EvidenceArtifactKind::ReleaseManifest)
    )
}

fn current_artifact(
    evidence: &SecurityEvidence,
    target: EvidenceArtifactKind,
    candidate: IntegratedCandidate,
) -> (result: Option<(usize, &crate::ArtifactObservation)>)
    ensures match result {
        Some((index, observation)) => {
            &&& model::first_observation_at(
                evidence.spec_artifacts(), index as int, target, candidate)
            &&& *observation == evidence.spec_artifacts()[index as int]
        },
        None => forall |index: int| 0 <= index < evidence.spec_artifacts().len() ==>
            !model::observation_matches(
                #[trigger] evidence.spec_artifacts()[index], target, candidate),
    },
{
    let values = evidence.artifacts();
    let mut index = 0;
    while index < values.len()
        invariant
            0 <= index <= values.len(),
            values@ == evidence.spec_artifacts(),
            forall |prior: int| 0 <= prior < index ==>
                !model::observation_matches(#[trigger] values@[prior], target, candidate),
        decreases values.len() - index,
    {
        if kinds_equal(values[index].kind(), target)
            && crate::binding::candidate_matches(values[index].candidate(), candidate)
        {
            return Some((index, &values[index]));
        }
        index += 1;
    }
    None
}

pub(super) fn evaluate(
    candidate: IntegratedCandidate,
    evidence: &SecurityEvidence,
    unmet: &mut Vec<UnmetSecurityCondition>,
) -> (complete: bool)
    ensures
        complete == evidence_complete(evidence, candidate),
        complete ==> final(unmet)@ == old(unmet)@,
{
    let mut complete = true;
    let mut index = 0;
    while index < EvidenceArtifactKind::ALL.len()
        invariant
            0 <= index <= EvidenceArtifactKind::ALL.len(),
            complete == model::complete_through(evidence, candidate, index as int),
            complete ==> unmet@ == old(unmet)@,
        decreases EvidenceArtifactKind::ALL.len() - index,
    {
        let kind = EvidenceArtifactKind::ALL[index];
        match current_artifact(evidence, kind, candidate) {
            None => {
                complete = false;
                unmet.push(UnmetSecurityCondition::MissingEvidenceArtifact(kind));
            }
            Some((_observation_index, observation)) => {
                if !crate::binding::digest_present(observation.digest()) {
                    complete = false;
                    unmet.push(UnmetSecurityCondition::EmptyEvidenceDigest(kind));
                }
                proof {
                    model::admission_at_first(
                        evidence.spec_artifacts(),
                        _observation_index as int,
                        kind,
                        candidate,
                    );
                };
            }
        }
        proof {
            model::complete_step(evidence, candidate, index as int);
        }
        index += 1;
    }
    reveal(evidence_complete);
    reveal(model::complete);
    complete
}

} // verus!
