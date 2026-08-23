//! One-field-at-a-time request digest mutation evidence.

use peritus_policy::{
    ActorRole, ApprovalRequirement, AuthorityInstant, AuthorityTier, AuthorityTimeState,
    CapabilityScope, IndependenceRequirement, IndependenceSet, Permission, PermissionSet,
    RiskClass, RiskSet, UseLimit, ValidityWindow,
};
use peritus_types::{
    AcceptanceSpecId, ActionId, ActorId, ApprovalRequestId, EnvironmentId, Generation, HarnessId,
    PolicyId, ProviderProfileId, ResourceId, RevisionNumber, RevisionTuple, Sha256Digest,
    WorkspaceId,
};

#[derive(Clone, Copy, Eq, PartialEq)]
enum Field {
    RequestId,
    ActionId,
    ActionDigest,
    Requester,
    RequesterRole,
    ScopeActor,
    ScopeRole,
    Environment,
    Permissions,
    Revision,
    ScopeValidity,
    UseLimit,
    MinimumTier,
    ApproverRoles,
    Independence,
    RequirementValidity,
    EvaluatedAt,
    ChallengeFloor,
    Risks,
    RiskDetailsDigest,
    ProducingParticipants,
    ReviewParticipants,
    RequestValidity,
}

fn instant(tick: u64) -> AuthorityInstant {
    AuthorityInstant::new(Generation::first(), tick)
}

fn window(start: u64, end: u64) -> ValidityWindow {
    ValidityWindow::new(instant(start), instant(end)).expect("valid test window")
}

fn identifier<const BYTE: u8>() -> [u8; 16] {
    [BYTE; 16]
}

fn selected(field: Option<Field>, candidate: Field) -> bool {
    field == Some(candidate)
}

fn revision(field: Option<Field>) -> RevisionTuple {
    let workspace = WorkspaceId::new(identifier::<4>()).expect("workspace");
    RevisionTuple::new(
        AcceptanceSpecId::new(identifier::<5>()).expect("acceptance"),
        HarnessId::new(identifier::<6>()).expect("harness"),
        workspace,
        Generation::first(),
        RevisionNumber::first(),
        PolicyId::new(if selected(field, Field::Revision) {
            identifier::<24>()
        } else {
            identifier::<8>()
        })
        .expect("policy"),
        ProviderProfileId::new(identifier::<9>()).expect("provider"),
    )
}

fn scope(field: Option<Field>) -> CapabilityScope {
    let capability = peritus_types::CapabilityName::new(
        if selected(field, Field::Permissions) { "workspace.mutate" } else { "workspace.inspect" }
            .to_owned(),
    )
    .expect("capability");
    let permissions = PermissionSet::new(vec![Permission::new(
        ResourceId::new(if selected(field, Field::Permissions) {
            identifier::<25>()
        } else {
            identifier::<12>()
        })
        .expect("resource"),
        capability,
    )])
    .expect("permissions");
    CapabilityScope::new(
        ActorId::new(if selected(field, Field::ScopeActor) {
            identifier::<26>()
        } else {
            identifier::<1>()
        })
        .expect("scope actor"),
        if selected(field, Field::ScopeRole) { ActorRole::Fixer } else { ActorRole::Writer },
        EnvironmentId::new(if selected(field, Field::Environment) {
            identifier::<27>()
        } else {
            identifier::<3>()
        })
        .expect("environment"),
        permissions,
        revision(field),
        if selected(field, Field::ScopeValidity) { window(5, 94) } else { window(5, 95) },
        if selected(field, Field::UseLimit) {
            UseLimit::limited(2).expect("limit")
        } else {
            UseLimit::limited(1).expect("limit")
        },
    )
}

fn requirement(field: Option<Field>) -> ApprovalRequirement {
    ApprovalRequirement::new(
        if selected(field, Field::MinimumTier) {
            AuthorityTier::Organization
        } else {
            AuthorityTier::User
        },
        vec![if selected(field, Field::ApproverRoles) {
            ActorRole::Reviewer
        } else {
            ActorRole::HumanAuthority
        }],
        IndependenceSet::new(if selected(field, Field::Independence) {
            vec![IndependenceRequirement::NotRequester]
        } else {
            Vec::new()
        })
        .expect("independence"),
        if selected(field, Field::RequirementValidity) { window(10, 89) } else { window(10, 90) },
    )
    .expect("requirement")
}

fn participants(field: Option<Field>) -> (crate::ParticipantSet, crate::ParticipantSet) {
    let producing =
        crate::ParticipantSet::producing(if selected(field, Field::ProducingParticipants) {
            vec![ActorId::new(identifier::<28>()).expect("producer")]
        } else {
            Vec::new()
        })
        .expect("producing participants");
    let review = crate::ParticipantSet::review(if selected(field, Field::ReviewParticipants) {
        vec![ActorId::new(identifier::<29>()).expect("reviewer")]
    } else {
        Vec::new()
    })
    .expect("review participants");
    (producing, review)
}

fn request(field: Option<Field>) -> crate::ApprovalRequest {
    let (producing_participants, review_participants) = participants(field);
    let mut request = crate::ApprovalRequest {
        request_id: ApprovalRequestId::new(if selected(field, Field::RequestId) {
            identifier::<30>()
        } else {
            identifier::<10>()
        })
        .expect("request"),
        action_id: ActionId::new(if selected(field, Field::ActionId) {
            identifier::<31>()
        } else {
            identifier::<11>()
        })
        .expect("action"),
        action_digest: crate::ActionDigest::from_sha256(Sha256Digest::new(
            if selected(field, Field::ActionDigest) { [32; 32] } else { [14; 32] },
        )),
        requester: ActorId::new(if selected(field, Field::Requester) {
            identifier::<33>()
        } else {
            identifier::<1>()
        })
        .expect("requester"),
        requester_role: if selected(field, Field::RequesterRole) {
            ActorRole::Fixer
        } else {
            ActorRole::Writer
        },
        scope: scope(field),
        requirement: requirement(field),
        evaluated_at: instant(if selected(field, Field::EvaluatedAt) { 21 } else { 20 }),
        challenge_epoch: Generation::first(),
        challenge_tick_millis: if selected(field, Field::ChallengeFloor) { 21 } else { 20 },
        authority_time: AuthorityTimeState::new(instant(20)),
        risks: RiskSet::new(vec![if selected(field, Field::Risks) {
            RiskClass::ScopedWrite
        } else {
            RiskClass::Read
        }])
        .expect("risks"),
        risk_details_digest: Sha256Digest::new(if selected(field, Field::RiskDetailsDigest) {
            [34; 32]
        } else {
            [15; 32]
        }),
        producing_participants,
        review_participants,
        validity: if selected(field, Field::RequestValidity) {
            window(10, 89)
        } else {
            window(10, 90)
        },
        digest: crate::ApprovalRequestDigest::from_sha256(Sha256Digest::new([0; 32])),
    };
    request.digest = crate::ApprovalRequestDigest::compute(&request).expect("bounded digest");
    request
}

#[test]
fn every_request_preimage_field_changes_the_digest_in_isolation() {
    let baseline = request(None).digest();
    let variants = [
        ("request_id", Field::RequestId),
        ("action_id", Field::ActionId),
        ("action_digest", Field::ActionDigest),
        ("requester", Field::Requester),
        ("requester_role", Field::RequesterRole),
        ("scope_actor", Field::ScopeActor),
        ("scope_role", Field::ScopeRole),
        ("environment", Field::Environment),
        ("permissions", Field::Permissions),
        ("revision", Field::Revision),
        ("scope_validity", Field::ScopeValidity),
        ("use_limit", Field::UseLimit),
        ("minimum_tier", Field::MinimumTier),
        ("approver_roles", Field::ApproverRoles),
        ("independence", Field::Independence),
        ("requirement_validity", Field::RequirementValidity),
        ("evaluated_at", Field::EvaluatedAt),
        ("challenge_floor", Field::ChallengeFloor),
        ("risks", Field::Risks),
        ("risk_details_digest", Field::RiskDetailsDigest),
        ("producing_participants", Field::ProducingParticipants),
        ("review_participants", Field::ReviewParticipants),
        ("request_validity", Field::RequestValidity),
    ];
    for (name, field) in variants {
        assert_ne!(baseline, request(Some(field)).digest(), "request field {name}");
    }
}
