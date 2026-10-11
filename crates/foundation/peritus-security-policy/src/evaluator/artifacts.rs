//! Required canonical evidence-manifest role checks.

mod model;

use crate::{EvidenceArtifactKind, IntegratedCandidate, SecurityEvidence, UnmetSecurityCondition};

const fn kinds_equal(left: EvidenceArtifactKind, right: EvidenceArtifactKind) -> bool {
    matches!(
        (left, right),
        (EvidenceArtifactKind::CampaignPlan, EvidenceArtifactKind::CampaignPlan)
            | (EvidenceArtifactKind::NativeProbeResults, EvidenceArtifactKind::NativeProbeResults)
            | (EvidenceArtifactKind::ResourceAccounting, EvidenceArtifactKind::ResourceAccounting)
            | (EvidenceArtifactKind::CleanupLedger, EvidenceArtifactKind::CleanupLedger)
            | (
                EvidenceArtifactKind::ThreatControlInventory,
                EvidenceArtifactKind::ThreatControlInventory
            )
            | (EvidenceArtifactKind::UnsafeTcbInventory, EvidenceArtifactKind::UnsafeTcbInventory)
            | (
                EvidenceArtifactKind::ExternalReviewReport,
                EvidenceArtifactKind::ExternalReviewReport
            )
            | (EvidenceArtifactKind::FindingRegister, EvidenceArtifactKind::FindingRegister)
            | (
                EvidenceArtifactKind::SupplyChainAttestation,
                EvidenceArtifactKind::SupplyChainAttestation
            )
            | (EvidenceArtifactKind::ReleaseManifest, EvidenceArtifactKind::ReleaseManifest)
    )
}
fn current_artifact(
    evidence: &SecurityEvidence,
    target: EvidenceArtifactKind,
    candidate: IntegratedCandidate,
) -> Option<(usize, &crate::ArtifactObservation)> {
    let values = evidence.artifacts();
    let mut index = 0;
    while index < values.len() {
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
) -> bool {
    let mut complete = true;
    let mut index = 0;
    while index < EvidenceArtifactKind::ALL.len() {
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
            }
        }
        index += 1;
    }
    complete
}
