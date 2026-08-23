use peritus_approval::AmendmentIdentity;
use peritus_policy::{
    IndependenceRequirement, OperationClass, PolicyAmendmentProposal, PolicyRevisionCandidate,
    PolicyTier, RestrictionLayer, RiskClass,
};
use peritus_types::{PolicyId, Sha256Digest};

use super::{ids, request_fixture::policy_definition};

pub fn amendment_candidate() -> (PolicyRevisionCandidate, AmendmentIdentity) {
    amendment_candidate_with(18, 19)
}

pub fn amendment_candidate_with(
    successor_byte: u8,
    digest_byte: u8,
) -> (PolicyRevisionCandidate, AmendmentIdentity) {
    let ids = ids();
    let successor = PolicyId::new([successor_byte; 16]).expect("successor policy");
    let digest = Sha256Digest::new([digest_byte; 32]);
    let tier = PolicyTier::Project;
    let proposal = PolicyAmendmentProposal::new(
        ids.revision.policy_id(),
        successor,
        tier,
        RestrictionLayer::new(tier, Vec::new()).expect("empty replacement layer"),
        digest,
    )
    .expect("amendment proposal");
    let candidate = policy_definition(
        1,
        vec![IndependenceRequirement::NotRequester],
        OperationClass::Inspection,
        vec![RiskClass::Read],
    )
    .preview_amendment(&proposal)
    .expect("exact policy candidate");
    let identity = AmendmentIdentity::new(ids.revision.policy_id(), successor, tier, digest)
        .expect("distinct amendment identities");
    (candidate, identity)
}
