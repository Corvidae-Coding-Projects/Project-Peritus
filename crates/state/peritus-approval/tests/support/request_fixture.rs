//! Policy-derived approval request fixtures, including isolated validity bounds.

use peritus_approval::{ActionDigest, ApprovalRequest, ParticipantSet};
use peritus_policy::{
    ActorRole, ActorSelector, ApprovalRequirement, AuthorityBoundary, AuthorityCeiling,
    AuthorityTimeState, CapabilityScope, CeilingGrant, EnvironmentSelector,
    IndependenceRequirement, IndependenceSet, OperationClass, OperationDescriptor,
    OperationRegistry, PermissionSelector, PolicyDefinition, PolicyTier, RestrictionLayer,
    RestrictionRule, RiskClass, RiskSet, RoleSelector, ScopeSelector, UseLimit,
};
use peritus_types::{ActorId, Sha256Digest};

use super::{ids, instant, permissions, window};

fn requirement(
    independence: Vec<IndependenceRequirement>,
    requirement_end: u64,
) -> ApprovalRequirement {
    ApprovalRequirement::new(
        peritus_policy::AuthorityTier::User,
        vec![ActorRole::HumanAuthority],
        IndependenceSet::new(independence).expect("canonical independence"),
        window(10, requirement_end),
    )
    .expect("approval requirement")
}

fn requested_scope(permission_count: usize, scope_end: u64) -> CapabilityScope {
    let ids = ids();
    CapabilityScope::new(
        ids.requester,
        ActorRole::Writer,
        ids.environment,
        permissions(permission_count),
        ids.revision,
        window(5, scope_end),
        UseLimit::limited(1).expect("positive use limit"),
    )
}

pub(super) fn policy_definition(
    permission_count: usize,
    independence: Vec<IndependenceRequirement>,
    operation: OperationClass,
    risks: Vec<RiskClass>,
) -> PolicyDefinition {
    policy_definition_with_bounds(permission_count, independence, operation, risks, 95, 90)
}

fn policy_definition_with_bounds(
    permission_count: usize,
    independence: Vec<IndependenceRequirement>,
    operation: OperationClass,
    risks: Vec<RiskClass>,
    scope_end: u64,
    requirement_end: u64,
) -> PolicyDefinition {
    let ids = ids();
    let boundary = AuthorityBoundary::new(
        vec![ids.requester],
        vec![ActorRole::Writer],
        vec![ids.environment],
        permissions(permission_count),
        ids.revision,
        window(0, 100),
        UseLimit::limited(2).expect("positive use limit"),
    )
    .expect("authority boundary");
    let selector = ScopeSelector::new(
        ActorSelector::any_within_parent(),
        RoleSelector::any_within_parent(),
        EnvironmentSelector::any_within_parent(),
        PermissionSelector::any_within_parent(),
        ids.revision,
    );
    let ceiling = AuthorityCeiling::new(
        boundary,
        vec![CeilingGrant::new(
            Sha256Digest::new([12; 32]),
            selector,
            window(5, scope_end),
            UseLimit::limited(1).expect("positive use limit"),
        )],
        Vec::new(),
    )
    .expect("authority ceiling");
    let descriptor = OperationDescriptor::new(
        super::capability_name(),
        operation,
        RiskSet::new(risks).expect("risk set"),
    )
    .expect("operation descriptor");
    let registry = OperationRegistry::new(vec![descriptor]).expect("operation registry");
    let approval_selector = ScopeSelector::new(
        ActorSelector::any_within_parent(),
        RoleSelector::any_within_parent(),
        EnvironmentSelector::any_within_parent(),
        PermissionSelector::any_within_parent(),
        ids.revision,
    );
    let layer = RestrictionLayer::new(
        PolicyTier::Project,
        vec![RestrictionRule::require_approval(
            Sha256Digest::new([13; 32]),
            approval_selector,
            requirement(independence, requirement_end),
        )],
    )
    .expect("restriction layer");
    PolicyDefinition::new(ids.revision.policy_id(), ceiling, registry, vec![layer]).expect("policy")
}

fn challenge(
    permission_count: usize,
    independence: Vec<IndependenceRequirement>,
    operation: OperationClass,
    risks: Vec<RiskClass>,
    scope_end: u64,
    requirement_end: u64,
) -> peritus_policy::EscalationChallenge {
    let policy = policy_definition_with_bounds(
        permission_count,
        independence,
        operation,
        risks,
        scope_end,
        requirement_end,
    );
    let decision = policy
        .evaluate(
            peritus_policy::AuthorizationRequest::new(requested_scope(permission_count, scope_end)),
            AuthorityTimeState::new(instant(0)),
            instant(20),
        )
        .expect("monotonic evaluation");
    let (_, challenge, _) = decision.into_parts();
    challenge.expect("approval-required policy decision")
}

pub fn challenge_with_operation_risks(
    permission_count: usize,
    independence: Vec<IndependenceRequirement>,
    operation: OperationClass,
    risks: Vec<RiskClass>,
) -> peritus_policy::EscalationChallenge {
    challenge(permission_count, independence, operation, risks, 95, 90)
}

pub fn request(
    permission_count: usize,
    independence: Vec<IndependenceRequirement>,
) -> ApprovalRequest {
    request_result(permission_count, independence).expect("approval request")
}

pub fn request_result(
    permission_count: usize,
    independence: Vec<IndependenceRequirement>,
) -> Result<ApprovalRequest, peritus_approval::ApprovalError> {
    request_result_with_risk_digest(permission_count, independence, Sha256Digest::new([15; 32]))
}

pub fn request_with_risk_digest(
    permission_count: usize,
    independence: Vec<IndependenceRequirement>,
    risk_details_digest: Sha256Digest,
) -> ApprovalRequest {
    request_result_with_risk_digest(permission_count, independence, risk_details_digest)
        .expect("approval request")
}

pub fn request_result_with_risk_digest(
    permission_count: usize,
    independence: Vec<IndependenceRequirement>,
    risk_details_digest: Sha256Digest,
) -> Result<ApprovalRequest, peritus_approval::ApprovalError> {
    request_from_parts(
        permission_count,
        independence,
        Vec::new(),
        Vec::new(),
        risk_details_digest,
        95,
        90,
        90,
    )
}

pub fn request_with_participants(
    independence: Vec<IndependenceRequirement>,
    producing: Vec<ActorId>,
    review: Vec<ActorId>,
) -> ApprovalRequest {
    request_with_permission_and_participants(1, independence, producing, review)
}

pub fn request_with_permission_and_participants(
    permission_count: usize,
    independence: Vec<IndependenceRequirement>,
    producing: Vec<ActorId>,
    review: Vec<ActorId>,
) -> ApprovalRequest {
    request_from_parts(
        permission_count,
        independence,
        producing,
        review,
        Sha256Digest::new([15; 32]),
        95,
        90,
        90,
    )
    .expect("approval request")
}

#[allow(clippy::too_many_arguments)]
fn request_from_parts(
    permission_count: usize,
    independence: Vec<IndependenceRequirement>,
    producing: Vec<ActorId>,
    review: Vec<ActorId>,
    risk_details_digest: Sha256Digest,
    scope_end: u64,
    requirement_end: u64,
    request_end: u64,
) -> Result<ApprovalRequest, peritus_approval::ApprovalError> {
    let ids = ids();
    ApprovalRequest::new(
        ids.request,
        ids.action,
        ActionDigest::from_sha256(Sha256Digest::new([14; 32])),
        ids.requester,
        ActorRole::Writer,
        challenge(
            permission_count,
            independence,
            OperationClass::Inspection,
            vec![RiskClass::Read],
            scope_end,
            requirement_end,
        ),
        risk_details_digest,
        ParticipantSet::producing(producing).expect("canonical producing participants"),
        ParticipantSet::review(review).expect("canonical review participants"),
        window(10, request_end),
    )
}
