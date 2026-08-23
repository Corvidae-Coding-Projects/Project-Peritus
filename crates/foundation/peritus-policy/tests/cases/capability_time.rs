use crate::support::{
    FixtureIds, PermissionSpec, PolicyInput, ScopeInput, action, command, descriptor, digest,
    grant, permission, permission_selector, policy, registry, scope, use_limit, window,
};
use peritus_policy::{
    ActorRole, AuthorityTimeState, AuthorizationDenialReason, AuthorizationRequest, Capability,
    CapabilityUseRequest, OperationClass, PolicyDecisionKind, PolicyErrorKind, ScopeDimension,
};

const fn permission_spec(ids: &FixtureIds) -> PermissionSpec {
    PermissionSpec { resource: ids.first_resource, name: "workspace.mutate" }
}

fn policy_and_scope(
    ids: &FixtureIds,
    uses: u64,
) -> (peritus_policy::PolicyDefinition, peritus_policy::CapabilityScope) {
    let permission = permission_spec(ids);
    let revision = ids.revision();
    let validity = window(1, 10, 100);
    let policy = policy(PolicyInput {
        actors: vec![ids.actor],
        roles: vec![ActorRole::Writer],
        environments: vec![ids.environment],
        permissions: vec![permission],
        revision,
        validity,
        uses: use_limit(Some(uses)),
        grants: vec![grant(
            10,
            permission_selector(revision, vec![permission]),
            validity,
            use_limit(Some(uses)),
        )],
        immutable_denies: Vec::new(),
        operations: registry(vec![descriptor(
            "workspace.mutate",
            OperationClass::WorkspaceMutation,
        )]),
        layers: Vec::new(),
    });
    let scope = scope(ScopeInput {
        actor: ids.actor,
        role: ActorRole::Writer,
        environment: ids.environment,
        permissions: vec![permission],
        revision,
        validity,
        uses: use_limit(Some(uses)),
    });
    (policy, scope)
}

fn issue(ids: &FixtureIds, uses: u64) -> Capability {
    let (policy, scope) = policy_and_scope(ids, uses);
    let decision = policy
        .evaluate(
            AuthorizationRequest::new(scope),
            AuthorityTimeState::new(crate::support::instant(1, 0)),
            crate::support::instant(1, 10),
        )
        .expect("evaluation");
    let (plan, challenge, denial) = decision.into_parts();
    assert!(challenge.is_none());
    assert!(denial.is_none());
    plan.expect("issuance plan").issue(command(1), digest(2)).into_capability()
}

struct UseInput {
    action_byte: u8,
    action_digest_byte: u8,
    actor: peritus_types::ActorId,
    role: ActorRole,
    environment: peritus_types::EnvironmentId,
    revision: peritus_types::RevisionTuple,
    permission: PermissionSpec,
    tick: u64,
}

fn use_request(input: &UseInput) -> CapabilityUseRequest {
    CapabilityUseRequest::new(
        action(input.action_byte),
        digest(input.action_digest_byte),
        permission(input.permission),
        input.actor,
        input.role,
        input.environment,
        input.revision,
        crate::support::instant(1, input.tick),
    )
}

fn valid_use(ids: &FixtureIds, tick: u64, action_byte: u8) -> CapabilityUseRequest {
    use_request(&UseInput {
        action_byte,
        action_digest_byte: action_byte + 20,
        actor: ids.actor,
        role: ActorRole::Writer,
        environment: ids.environment,
        revision: ids.revision(),
        permission: permission_spec(ids),
        tick,
    })
}

#[test]
fn issuance_preserves_exact_scope_command_digest_use_and_time() {
    let ids = FixtureIds::new();
    let (policy, scope) = policy_and_scope(&ids, 3);
    let decision = policy
        .evaluate(
            AuthorizationRequest::new(scope),
            AuthorityTimeState::new(crate::support::instant(1, 0)),
            crate::support::instant(1, 10),
        )
        .expect("evaluation");
    let plan = decision.into_parts().0.expect("plan");
    let transition = plan.issue(command(7), digest(8));
    assert_eq!(transition.command_id(), command(7));
    assert_eq!(transition.transition_digest(), digest(8));
    let capability = transition.capability();
    assert_eq!(capability.scope().actor_id(), ids.actor);
    assert_eq!(capability.scope().role(), ActorRole::Writer);
    assert_eq!(capability.scope().environment_id(), ids.environment);
    assert_eq!(capability.scope().revision(), ids.revision());
    assert!(capability.scope().contains_permission(
        ids.first_resource,
        &peritus_types::CapabilityName::new("workspace.mutate".to_owned()).expect("name")
    ));
    assert_eq!(capability.issued_at(), crate::support::instant(1, 10));
    assert_eq!(capability.issuance_command_id(), command(7));
    assert_eq!(capability.issuance_digest(), digest(8));
    assert_eq!(capability.remaining_uses().remaining(), Some(3));
    assert_eq!(capability.time_state().greatest_tick_millis(), 10);
}

#[test]
fn successful_uses_bind_exact_actions_and_decrement_once_until_exhausted() {
    let ids = FixtureIds::new();
    let capability = issue(&ids, 2);
    let first = capability.try_use(valid_use(&ids, 20, 10), digest(40)).expect("first use");
    assert_eq!(first.action_id(), action(10));
    assert_eq!(first.action_digest(), digest(30));
    assert_eq!(first.transition_digest(), digest(40));
    assert_eq!(first.permission(), &permission(permission_spec(&ids)));
    assert_eq!(first.used_at(), crate::support::instant(1, 20));
    assert_eq!(first.previous_remaining().remaining(), Some(2));
    assert_eq!(first.successor().remaining_uses().remaining(), Some(1));

    let second =
        first.into_successor().try_use(valid_use(&ids, 30, 11), digest(41)).expect("second use");
    assert_eq!(second.previous_remaining().remaining(), Some(1));
    assert_eq!(second.successor().remaining_uses().remaining(), Some(0));
    let exhausted = second
        .into_successor()
        .try_use(valid_use(&ids, 40, 12), digest(42))
        .expect_err("exhausted");
    assert_eq!(exhausted.error().kind(), PolicyErrorKind::CapabilityExhausted);
    assert_eq!(exhausted.capability().remaining_uses().remaining(), Some(0));
}

#[test]
fn every_scope_mismatch_preserves_the_complete_capability() {
    let ids = FixtureIds::new();
    let mut capability = issue(&ids, 2);
    let cases = [
        (
            UseInput {
                action_byte: 1,
                action_digest_byte: 21,
                actor: ids.other_actor,
                role: ActorRole::Writer,
                environment: ids.environment,
                revision: ids.revision(),
                permission: permission_spec(&ids),
                tick: 20,
            },
            ScopeDimension::Actor,
        ),
        (
            UseInput {
                action_byte: 2,
                action_digest_byte: 22,
                actor: ids.actor,
                role: ActorRole::Reviewer,
                environment: ids.environment,
                revision: ids.revision(),
                permission: permission_spec(&ids),
                tick: 20,
            },
            ScopeDimension::Role,
        ),
        (
            UseInput {
                action_byte: 3,
                action_digest_byte: 23,
                actor: ids.actor,
                role: ActorRole::Writer,
                environment: ids.other_environment,
                revision: ids.revision(),
                permission: permission_spec(&ids),
                tick: 20,
            },
            ScopeDimension::Environment,
        ),
        (
            UseInput {
                action_byte: 4,
                action_digest_byte: 24,
                actor: ids.actor,
                role: ActorRole::Writer,
                environment: ids.environment,
                revision: peritus_types::RevisionTuple::new(
                    ids.revision().acceptance_spec_id(),
                    ids.revision().harness_id(),
                    ids.revision().workspace_id(),
                    ids.revision().workspace_generation(),
                    ids.revision().workspace_revision(),
                    ids.other_policy,
                    ids.revision().provider_profile_id(),
                ),
                permission: permission_spec(&ids),
                tick: 20,
            },
            ScopeDimension::Revision,
        ),
        (
            UseInput {
                action_byte: 5,
                action_digest_byte: 25,
                actor: ids.actor,
                role: ActorRole::Writer,
                environment: ids.environment,
                revision: ids.revision(),
                permission: PermissionSpec {
                    resource: ids.second_resource,
                    name: "workspace.mutate",
                },
                tick: 20,
            },
            ScopeDimension::Permissions,
        ),
    ];
    for (input, expected) in cases {
        let failure =
            capability.try_use(use_request(&input), digest(50)).expect_err("scope mismatch");
        assert_eq!(failure.error().kind(), PolicyErrorKind::CapabilityScopeMismatch);
        assert_eq!(failure.error().dimension(), Some(expected));
        assert_eq!(failure.capability().remaining_uses().remaining(), Some(2));
        assert_eq!(failure.capability().issuance_command_id(), command(1));
        assert_eq!(failure.capability().issuance_digest(), digest(2));
        assert_eq!(failure.capability().time_state().greatest_tick_millis(), 10);
        capability = failure.into_capability();
    }
    let success = capability
        .try_use(valid_use(&ids, 20, 6), digest(51))
        .expect("use after rejected attempts");
    assert_eq!(success.successor().remaining_uses().remaining(), Some(1));
}

#[test]
fn half_open_time_and_move_only_failures_preserve_the_floor() {
    let ids = FixtureIds::new();
    let (policy, scope) = policy_and_scope(&ids, 2);
    let at_start = policy
        .evaluate(
            AuthorizationRequest::new(scope),
            AuthorityTimeState::new(crate::support::instant(1, 0)),
            crate::support::instant(1, 10),
        )
        .expect("start equality");
    assert_eq!(at_start.kind(), PolicyDecisionKind::Authorized);

    let (_, expiry_scope) = policy_and_scope(&ids, 2);
    let at_expiry = policy
        .evaluate(
            AuthorizationRequest::new(expiry_scope),
            AuthorityTimeState::new(crate::support::instant(1, 0)),
            crate::support::instant(1, 100),
        )
        .expect("expiry decision");
    assert_eq!(
        at_expiry.denial().map(peritus_policy::AuthorizationDenial::reason),
        Some(AuthorizationDenialReason::Expired)
    );

    let regression = AuthorityTimeState::new(crate::support::instant(1, 20))
        .observe(crate::support::instant(1, 19))
        .expect_err("regression");
    assert_eq!(regression.error().kind(), PolicyErrorKind::ClockRegression);
    assert_eq!(regression.state().greatest_tick_millis(), 20);
    let recovered = regression.into_state();
    assert_eq!(
        recovered
            .observe(crate::support::instant(1, 21))
            .expect("recovered state")
            .greatest_tick_millis(),
        21
    );

    let epoch = AuthorityTimeState::new(crate::support::instant(1, 20))
        .observe(crate::support::instant(2, 21))
        .expect_err("epoch mismatch");
    assert_eq!(epoch.error().kind(), PolicyErrorKind::ClockEpochMismatch);
    assert_eq!(epoch.state().epoch(), peritus_types::Generation::first());
}

#[test]
fn expired_and_regressing_capability_uses_do_not_consume() {
    let ids = FixtureIds::new();
    let capability = issue(&ids, 2);
    let regression = capability.try_use(valid_use(&ids, 9, 1), digest(60)).expect_err("regression");
    assert_eq!(regression.error().kind(), PolicyErrorKind::ClockRegression);
    assert_eq!(regression.capability().remaining_uses().remaining(), Some(2));
    let expired = regression
        .into_capability()
        .try_use(valid_use(&ids, 100, 2), digest(61))
        .expect_err("expiry equality");
    assert_eq!(expired.error().kind(), PolicyErrorKind::CapabilityExpired);
    assert_eq!(expired.capability().remaining_uses().remaining(), Some(2));
    assert_eq!(expired.capability().time_state().greatest_tick_millis(), 10);
}
