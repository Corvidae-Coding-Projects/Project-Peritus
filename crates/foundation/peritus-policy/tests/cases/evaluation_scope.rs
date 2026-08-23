use crate::support::{
    FixtureIds, PermissionSpec, PolicyInput, ScopeInput, descriptor, grant, permission_selector,
    policy, registry, scope, use_limit, window,
};
use peritus_policy::{
    ActorRole, AuthorityTimeState, AuthorizationDenialReason, AuthorizationRequest,
    CanonicalCollection, PolicyDecision, PolicyDecisionKind, PolicyErrorKind,
};
use peritus_types::{Generation, RevisionNumber, RevisionTuple};

const fn inspect(ids: &FixtureIds) -> PermissionSpec {
    PermissionSpec { resource: ids.first_resource, name: "workspace.inspect" }
}

const fn mutate(ids: &FixtureIds) -> PermissionSpec {
    PermissionSpec { resource: ids.second_resource, name: "workspace.mutate" }
}

fn baseline_policy(ids: &FixtureIds) -> peritus_policy::PolicyDefinition {
    let revision = ids.revision();
    policy(PolicyInput {
        actors: vec![ids.actor],
        roles: vec![ActorRole::Writer],
        environments: vec![ids.environment],
        permissions: vec![inspect(ids), mutate(ids)],
        revision,
        validity: window(1, 0, 100),
        uses: use_limit(Some(10)),
        grants: vec![
            grant(
                10,
                permission_selector(revision, vec![inspect(ids)]),
                window(1, 5, 90),
                use_limit(Some(8)),
            ),
            grant(
                11,
                permission_selector(revision, vec![mutate(ids)]),
                window(1, 10, 80),
                use_limit(Some(6)),
            ),
        ],
        immutable_denies: Vec::new(),
        operations: registry(vec![
            descriptor("workspace.inspect", peritus_policy::OperationClass::Inspection),
            descriptor("workspace.mutate", peritus_policy::OperationClass::WorkspaceMutation),
        ]),
        layers: Vec::new(),
    })
}

fn request(
    actor: peritus_types::ActorId,
    role: ActorRole,
    environment: peritus_types::EnvironmentId,
    permissions: Vec<PermissionSpec>,
    revision: RevisionTuple,
    validity: peritus_policy::ValidityWindow,
    uses: peritus_policy::UseLimit,
) -> AuthorizationRequest {
    AuthorizationRequest::new(scope(ScopeInput {
        actor,
        role,
        environment,
        permissions,
        revision,
        validity,
        uses,
    }))
}

fn baseline_request(ids: &FixtureIds) -> AuthorizationRequest {
    request(
        ids.actor,
        ActorRole::Writer,
        ids.environment,
        vec![inspect(ids), mutate(ids)],
        ids.revision(),
        window(1, 0, 100),
        use_limit(Some(9)),
    )
}

fn evaluate(
    policy: &peritus_policy::PolicyDefinition,
    request: AuthorizationRequest,
    observed_tick: u64,
) -> PolicyDecision {
    policy
        .evaluate(
            request,
            AuthorityTimeState::new(crate::support::instant(1, 0)),
            crate::support::instant(1, observed_tick),
        )
        .expect("consistent authority time")
}

fn assert_denied(decision: &PolicyDecision, expected: AuthorizationDenialReason) {
    assert_eq!(decision.kind(), PolicyDecisionKind::Denied);
    assert_eq!(decision.denial().expect("denial").reason(), expected);
    assert!(decision.authorized_plan().is_none());
    assert!(decision.escalation_challenge().is_none());
}

#[test]
fn union_of_exact_grants_authorizes_the_complete_pair_set() {
    let ids = FixtureIds::new();
    let policy = baseline_policy(&ids);
    let decision = evaluate(&policy, baseline_request(&ids), 30);
    assert_eq!(decision.kind(), PolicyDecisionKind::Authorized);
    let plan = decision.authorized_plan().expect("plan");
    assert_eq!(plan.scope().permissions().as_slice().len(), 2);
    assert_eq!(plan.scope().validity(), window(1, 10, 80));
    assert_eq!(plan.scope().use_limit().remaining(), Some(6));
    assert_eq!(plan.time_state().greatest_tick_millis(), 30);
}

#[test]
fn permission_atoms_never_expand_to_a_cartesian_product() {
    let ids = FixtureIds::new();
    let policy = baseline_policy(&ids);
    let crossed = PermissionSpec { resource: ids.first_resource, name: "workspace.mutate" };
    let decision = evaluate(
        &policy,
        request(
            ids.actor,
            ActorRole::Writer,
            ids.environment,
            vec![crossed],
            ids.revision(),
            window(1, 0, 100),
            use_limit(Some(3)),
        ),
        30,
    );
    assert_denied(&decision, AuthorizationDenialReason::OutsideAuthorityBoundary);
}

#[test]
fn every_identity_scope_dimension_is_checked_independently() {
    let ids = FixtureIds::new();
    let policy = baseline_policy(&ids);
    let cases = [
        request(
            ids.other_actor,
            ActorRole::Writer,
            ids.environment,
            vec![inspect(&ids)],
            ids.revision(),
            window(1, 0, 100),
            use_limit(Some(3)),
        ),
        request(
            ids.actor,
            ActorRole::Reviewer,
            ids.environment,
            vec![inspect(&ids)],
            ids.revision(),
            window(1, 0, 100),
            use_limit(Some(3)),
        ),
        request(
            ids.actor,
            ActorRole::Writer,
            ids.other_environment,
            vec![inspect(&ids)],
            ids.revision(),
            window(1, 0, 100),
            use_limit(Some(3)),
        ),
        request(
            ids.actor,
            ActorRole::Writer,
            ids.environment,
            vec![PermissionSpec { resource: ids.third_resource, name: "workspace.inspect" }],
            ids.revision(),
            window(1, 0, 100),
            use_limit(Some(3)),
        ),
        request(
            ids.actor,
            ActorRole::Writer,
            ids.environment,
            vec![PermissionSpec { resource: ids.first_resource, name: "workspace.unknown" }],
            ids.revision(),
            window(1, 0, 100),
            use_limit(Some(3)),
        ),
    ];
    for (case, request) in cases.into_iter().enumerate() {
        let decision = evaluate(&policy, request, 30);
        assert_eq!(
            decision.denial().map(peritus_policy::AuthorizationDenial::reason),
            Some(AuthorizationDenialReason::OutsideAuthorityBoundary),
            "dimension case {case}"
        );
    }
}

fn revision_cases(ids: &FixtureIds) -> [(RevisionTuple, AuthorizationDenialReason); 7] {
    let base = ids.revision();
    [
        (
            RevisionTuple::new(
                ids.other_acceptance,
                base.harness_id(),
                base.workspace_id(),
                base.workspace_generation(),
                base.workspace_revision(),
                base.policy_id(),
                base.provider_profile_id(),
            ),
            AuthorizationDenialReason::OutsideAuthorityBoundary,
        ),
        (
            RevisionTuple::new(
                base.acceptance_spec_id(),
                ids.other_harness,
                base.workspace_id(),
                base.workspace_generation(),
                base.workspace_revision(),
                base.policy_id(),
                base.provider_profile_id(),
            ),
            AuthorizationDenialReason::OutsideAuthorityBoundary,
        ),
        (
            RevisionTuple::new(
                base.acceptance_spec_id(),
                base.harness_id(),
                ids.other_workspace,
                base.workspace_generation(),
                base.workspace_revision(),
                base.policy_id(),
                base.provider_profile_id(),
            ),
            AuthorizationDenialReason::OutsideAuthorityBoundary,
        ),
        (
            RevisionTuple::new(
                base.acceptance_spec_id(),
                base.harness_id(),
                base.workspace_id(),
                Generation::new(2).expect("generation"),
                base.workspace_revision(),
                base.policy_id(),
                base.provider_profile_id(),
            ),
            AuthorizationDenialReason::OutsideAuthorityBoundary,
        ),
        (
            RevisionTuple::new(
                base.acceptance_spec_id(),
                base.harness_id(),
                base.workspace_id(),
                base.workspace_generation(),
                RevisionNumber::new(2).expect("revision"),
                base.policy_id(),
                base.provider_profile_id(),
            ),
            AuthorizationDenialReason::OutsideAuthorityBoundary,
        ),
        (
            RevisionTuple::new(
                base.acceptance_spec_id(),
                base.harness_id(),
                base.workspace_id(),
                base.workspace_generation(),
                base.workspace_revision(),
                ids.other_policy,
                base.provider_profile_id(),
            ),
            AuthorizationDenialReason::PolicyMismatch,
        ),
        (
            RevisionTuple::new(
                base.acceptance_spec_id(),
                base.harness_id(),
                base.workspace_id(),
                base.workspace_generation(),
                base.workspace_revision(),
                base.policy_id(),
                ids.other_provider,
            ),
            AuthorizationDenialReason::OutsideAuthorityBoundary,
        ),
    ]
}

#[test]
fn every_revision_component_is_checked_independently() {
    let ids = FixtureIds::new();
    let policy = baseline_policy(&ids);
    for (case, (revision, expected)) in revision_cases(&ids).into_iter().enumerate() {
        let decision = evaluate(
            &policy,
            request(
                ids.actor,
                ActorRole::Writer,
                ids.environment,
                vec![inspect(&ids)],
                revision,
                window(1, 0, 100),
                use_limit(Some(3)),
            ),
            30,
        );
        assert_eq!(
            decision.denial().map(peritus_policy::AuthorizationDenial::reason),
            Some(expected),
            "revision component {case}"
        );
    }
}

#[test]
fn validity_and_use_dimensions_are_intersected_or_denied() {
    let ids = FixtureIds::new();
    let policy = baseline_policy(&ids);
    let not_yet = request(
        ids.actor,
        ActorRole::Writer,
        ids.environment,
        vec![inspect(&ids)],
        ids.revision(),
        window(1, 40, 70),
        use_limit(Some(2)),
    );
    assert_denied(&evaluate(&policy, not_yet, 30), AuthorizationDenialReason::NotYetValid);
    let expired = request(
        ids.actor,
        ActorRole::Writer,
        ids.environment,
        vec![inspect(&ids)],
        ids.revision(),
        window(1, 0, 30),
        use_limit(Some(1)),
    );
    assert_denied(&evaluate(&policy, expired, 30), AuthorizationDenialReason::Expired);
    assert_eq!(
        peritus_policy::PermissionSet::new(Vec::new()).expect_err("empty permissions").kind(),
        PolicyErrorKind::EmptyCanonicalCollection
    );
    assert_eq!(
        peritus_policy::PermissionSet::new(Vec::new()).expect_err("empty permissions").collection(),
        Some(CanonicalCollection::Permissions)
    );
}
