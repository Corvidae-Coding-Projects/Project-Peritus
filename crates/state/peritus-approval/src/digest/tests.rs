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

#[derive(Clone, Copy)]
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

fn request(field: Option<Field>) -> crate::ApprovalRequest {
    let selected = |candidate| {
        field.is_some_and(|value| {
            core::mem::discriminant(&value) == core::mem::discriminant(&candidate)
        })
    };
    let workspace = WorkspaceId::new(identifier::<4>()).expect("workspace");
    let revision = RevisionTuple::new(
        AcceptanceSpecId::new(identifier::<5>()).expect("acceptance"),
        HarnessId::new(identifier::<6>()).expect("harness"),
        workspace,
        Generation::first(),
        RevisionNumber::first(),
        PolicyId::new(if selected(Field::Revision) {
            identifier::<24>()
        } else {
            identifier::<8>()
        })
        .expect("policy"),
        ProviderProfileId::new(identifier::<9>()).expect("provider"),
    );
    let capability = peritus_types::CapabilityName::new(
        if selected(Field::Permissions) { "workspace.mutate" } else { "workspace.inspect" }
            .to_owned(),
    )
    .expect("capability");
    let permissions = PermissionSet::new(vec![Permission::new(
        ResourceId::new(if selected(Field::Permissions) {
            identifier::<25>()
        } else {
            identifier::<12>()
        })
        .expect("resource"),
        capability,
    )])
    .expect("permissions");
    let scope = CapabilityScope::new(
        ActorId::new(if selected(Field::ScopeActor) {
            identifier::<26>()
        } else {
            identifier::<1>()
        })
        .expect("scope actor"),
        if selected(Field::ScopeRole) { ActorRole::Fixer } else { ActorRole::Writer },
        EnvironmentId::new(if selected(Field::Environment) {
            identifier::<27>()
        } else {
            identifier::<3>()
        })
        .expect("environment"),
        permissions,
        revision,
        if selected(Field::ScopeValidity) { window(5, 94) } else { window(5, 95) },
        if selected(Field::UseLimit) {
            UseLimit::limited(2).expect("limit")
        } else {
            UseLimit::limited(1).expect("limit")
        },
    );
    let requirement = ApprovalRequirement::new(
        if selected(Field::MinimumTier) {
            AuthorityTier::Organization
        } else {
            AuthorityTier::User
        },
        vec![if selected(Field::ApproverRoles) {
            ActorRole::Reviewer
        } else {
            ActorRole::HumanAuthority
        }],
        IndependenceSet::new(if selected(Field::Independence) {
            vec![IndependenceRequirement::NotRequester]
        } else {
            Vec::new()
        })
        .expect("independence"),
        if selected(Field::RequirementValidity) { window(10, 89) } else { window(10, 90) },
    )
    .expect("requirement");
    let producing = crate::ParticipantSet::producing(if selected(Field::ProducingParticipants) {
        vec![ActorId::new(identifier::<28>()).expect("producer")]
    } else {
        Vec::new()
    })
    .expect("producing participants");
    let review = crate::ParticipantSet::review(if selected(Field::ReviewParticipants) {
        vec![ActorId::new(identifier::<29>()).expect("reviewer")]
    } else {
        Vec::new()
    })
    .expect("review participants");
    let mut request = crate::ApprovalRequest {
        request_id: ApprovalRequestId::new(if selected(Field::RequestId) {
            identifier::<30>()
        } else {
            identifier::<10>()
        })
        .expect("request"),
        action_id: ActionId::new(if selected(Field::ActionId) {
            identifier::<31>()
        } else {
            identifier::<11>()
        })
        .expect("action"),
        action_digest: crate::ActionDigest::from_sha256(Sha256Digest::new(
            if selected(Field::ActionDigest) { [32; 32] } else { [14; 32] },
        )),
        requester: ActorId::new(if selected(Field::Requester) {
            identifier::<33>()
        } else {
            identifier::<1>()
        })
        .expect("requester"),
        requester_role: if selected(Field::RequesterRole) {
            ActorRole::Fixer
        } else {
            ActorRole::Writer
        },
        scope,
        requirement,
        evaluated_at: instant(if selected(Field::EvaluatedAt) { 21 } else { 20 }),
        challenge_epoch: Generation::first(),
        challenge_tick_millis: if selected(Field::ChallengeFloor) { 21 } else { 20 },
        authority_time: AuthorityTimeState::new(instant(20)),
        risks: RiskSet::new(vec![if selected(Field::Risks) {
            RiskClass::ScopedWrite
        } else {
            RiskClass::Read
        }])
        .expect("risks"),
        risk_details_digest: Sha256Digest::new(if selected(Field::RiskDetailsDigest) {
            [34; 32]
        } else {
            [15; 32]
        }),
        producing_participants: producing,
        review_participants: review,
        validity: if selected(Field::RequestValidity) { window(10, 89) } else { window(10, 90) },
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
