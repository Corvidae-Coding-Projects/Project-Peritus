//! Cross-crate mandatory-risk binding evidence.

mod support;

use peritus_approval::{ActionDigest, ApprovalRequest, ParticipantSet};
use peritus_policy::{ActorRole, IndependenceRequirement, OperationClass, RiskClass};
use peritus_types::Sha256Digest;

fn request_for(risks: Vec<RiskClass>) -> ApprovalRequest {
    let ids = support::ids();
    ApprovalRequest::new(
        ids.request,
        ids.action,
        ActionDigest::from_sha256(Sha256Digest::new([14; 32])),
        ids.requester,
        ActorRole::Writer,
        support::challenge_with_operation_risks(
            1,
            vec![IndependenceRequirement::NotRequester],
            OperationClass::WorkspaceMutation,
            risks,
        ),
        Sha256Digest::new([33; 32]),
        ParticipantSet::producing(Vec::new()).expect("producing participants"),
        ParticipantSet::review(Vec::new()).expect("review participants"),
        support::window(10, 90),
    )
    .expect("approval request")
}

#[test]
fn request_has_no_caller_risk_slot_and_binds_the_exact_policy_union() {
    let mandatory = request_for(vec![RiskClass::ScopedWrite]);
    assert_eq!(mandatory.risks().as_slice(), &[RiskClass::ScopedWrite]);

    let with_authenticated_extra = request_for(vec![RiskClass::ScopedWrite, RiskClass::Network]);
    assert_eq!(
        with_authenticated_extra.risks().as_slice(),
        &[RiskClass::ScopedWrite, RiskClass::Network],
    );
    assert_ne!(mandatory.digest(), with_authenticated_extra.digest());
}
