//! Reusable downstream-style capability commit-claim fixtures.

use super::{
    FixtureIds, PermissionSpec, PolicyInput, ScopeInput, action, command, descriptor, digest,
    grant, instant, permission, permission_selector, policy, registry, scope, use_limit, window,
};
use peritus_policy::{
    ActorRole, AuthorityTimeState, AuthorizationRequest, Capability, CapabilityUseRequest,
    OperationClass, PolicyErrorKind, ScopeDimension,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapabilityCommitClaimKind {
    Forged,
    Malformed,
    Stale,
}

pub struct CapabilityCommitClaimFixture {
    pub kind: CapabilityCommitClaimKind,
    pub request: CapabilityUseRequest,
    pub expected_error: PolicyErrorKind,
    pub expected_dimension: Option<ScopeDimension>,
}

const fn permission_spec(ids: &FixtureIds) -> PermissionSpec {
    PermissionSpec { resource: ids.first_resource, name: "workspace.mutate" }
}

pub fn capability_for_commit_claim(ids: &FixtureIds) -> Capability {
    let permission = permission_spec(ids);
    let validity = window(1, 10, 100);
    let limit = use_limit(Some(2));
    let definition = policy(PolicyInput {
        actors: vec![ids.actor],
        roles: vec![ActorRole::Writer],
        environments: vec![ids.environment],
        permissions: vec![permission],
        revision: ids.revision(),
        validity,
        uses: limit,
        grants: vec![grant(
            10,
            permission_selector(ids.revision(), vec![permission]),
            validity,
            limit,
        )],
        immutable_denies: Vec::new(),
        operations: registry(vec![descriptor(
            "workspace.mutate",
            OperationClass::WorkspaceMutation,
        )]),
        layers: Vec::new(),
    });
    let requested = scope(ScopeInput {
        actor: ids.actor,
        role: ActorRole::Writer,
        environment: ids.environment,
        permissions: vec![permission],
        revision: ids.revision(),
        validity,
        uses: limit,
    });
    definition
        .evaluate(
            AuthorizationRequest::new(requested),
            AuthorityTimeState::new(instant(1, 0)),
            instant(1, 10),
        )
        .expect("claim fixture evaluation")
        .into_parts()
        .0
        .expect("claim fixture issuance")
        .issue(command(1), digest(2))
        .into_capability()
}

fn claim_request(
    ids: &FixtureIds,
    actor: peritus_types::ActorId,
    permission_spec: PermissionSpec,
    observed_tick: u64,
) -> CapabilityUseRequest {
    CapabilityUseRequest::new(
        action(7),
        digest(8),
        permission(permission_spec),
        actor,
        ActorRole::Writer,
        ids.environment,
        ids.revision(),
        instant(1, observed_tick),
    )
}

pub fn capability_commit_claim_fixtures(ids: &FixtureIds) -> Vec<CapabilityCommitClaimFixture> {
    vec![
        CapabilityCommitClaimFixture {
            kind: CapabilityCommitClaimKind::Forged,
            request: claim_request(ids, ids.other_actor, permission_spec(ids), 20),
            expected_error: PolicyErrorKind::CapabilityScopeMismatch,
            expected_dimension: Some(ScopeDimension::Actor),
        },
        CapabilityCommitClaimFixture {
            kind: CapabilityCommitClaimKind::Malformed,
            request: claim_request(
                ids,
                ids.actor,
                PermissionSpec { resource: ids.second_resource, name: "workspace.mutate" },
                20,
            ),
            expected_error: PolicyErrorKind::CapabilityScopeMismatch,
            expected_dimension: Some(ScopeDimension::Permissions),
        },
        CapabilityCommitClaimFixture {
            kind: CapabilityCommitClaimKind::Stale,
            request: claim_request(ids, ids.actor, permission_spec(ids), 9),
            expected_error: PolicyErrorKind::ClockRegression,
            expected_dimension: None,
        },
    ]
}
