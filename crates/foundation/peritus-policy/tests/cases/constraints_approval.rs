use crate::support::{
    FixtureIds, PermissionSpec, PolicyInput, ScopeInput, any_selector, approval_requirement,
    approval_rule, deny_rule, descriptor, grant, layer, permission_selector, policy, registry,
    scope, use_limit, window,
};
use peritus_policy::{
    ActorRole, ActorSelector, AuthorityTier, AuthorityTimeState, AuthorizationDenialReason,
    AuthorizationRequest, EnvironmentSelector, IndependenceRequirement, PermissionSelector,
    PolicyDecision, PolicyDecisionKind, PolicyTier, RoleSelector, ScopeSelector,
};

const fn permission(ids: &FixtureIds) -> PermissionSpec {
    PermissionSpec { resource: ids.first_resource, name: "workspace.inspect" }
}

fn request(ids: &FixtureIds) -> AuthorizationRequest {
    AuthorizationRequest::new(scope(ScopeInput {
        actor: ids.actor,
        role: ActorRole::Writer,
        environment: ids.environment,
        permissions: vec![permission(ids)],
        revision: ids.revision(),
        validity: window(1, 0, 100),
        uses: use_limit(Some(9)),
    }))
}

fn evaluate(policy: &peritus_policy::PolicyDefinition, ids: &FixtureIds) -> PolicyDecision {
    policy
        .evaluate(
            request(ids),
            AuthorityTimeState::new(crate::support::instant(1, 0)),
            crate::support::instant(1, 30),
        )
        .expect("authority time")
}

fn policy_with(
    ids: &FixtureIds,
    grants: Vec<peritus_policy::CeilingGrant>,
    immutable_denies: Vec<peritus_policy::RestrictionRule>,
    layers: Vec<peritus_policy::RestrictionLayer>,
) -> peritus_policy::PolicyDefinition {
    policy(PolicyInput {
        actors: vec![ids.actor, ids.other_actor],
        roles: vec![ActorRole::Writer],
        environments: vec![ids.environment],
        permissions: vec![permission(ids)],
        revision: ids.revision(),
        validity: window(1, 0, 100),
        uses: use_limit(Some(10)),
        grants,
        immutable_denies,
        operations: registry(vec![descriptor(
            "workspace.inspect",
            peritus_policy::OperationClass::Inspection,
        )]),
        layers,
    })
}

fn base_grant(ids: &FixtureIds) -> peritus_policy::CeilingGrant {
    grant(
        10,
        permission_selector(ids.revision(), vec![permission(ids)]),
        window(1, 5, 90),
        use_limit(Some(8)),
    )
}

fn assert_denial(decision: &PolicyDecision, reason: AuthorizationDenialReason) {
    assert_eq!(decision.kind(), PolicyDecisionKind::Denied);
    assert_eq!(decision.denial().expect("denial").reason(), reason);
}

#[test]
fn all_applicable_grants_constrain_even_when_coverage_is_already_complete() {
    let ids = FixtureIds::new();
    let policy = policy_with(
        &ids,
        vec![
            base_grant(&ids),
            grant(
                11,
                permission_selector(ids.revision(), vec![permission(&ids)]),
                window(1, 20, 70),
                use_limit(Some(4)),
            ),
        ],
        Vec::new(),
        Vec::new(),
    );
    let decision = evaluate(&policy, &ids);
    let plan = decision.authorized_plan().expect("authorized");
    assert_eq!(plan.scope().validity(), window(1, 20, 70));
    assert_eq!(plan.scope().use_limit().remaining(), Some(4));
}

#[test]
fn nonapplicable_grants_are_neutral_but_empty_applicable_intersections_deny() {
    let ids = FixtureIds::new();
    let nonmatching = ScopeSelector::new(
        ActorSelector::exact(vec![ids.other_actor]).expect("actor selector"),
        RoleSelector::any_within_parent(),
        EnvironmentSelector::any_within_parent(),
        PermissionSelector::any_within_parent(),
        ids.revision(),
    );
    let neutral_policy = policy_with(
        &ids,
        vec![base_grant(&ids), grant(11, nonmatching, window(1, 40, 60), use_limit(Some(1)))],
        Vec::new(),
        Vec::new(),
    );
    let neutral_decision = evaluate(&neutral_policy, &ids);
    let plan = neutral_decision.authorized_plan().expect("nonmatching grant neutral");
    assert_eq!(plan.scope().validity(), window(1, 5, 90));
    assert_eq!(plan.scope().use_limit().remaining(), Some(8));

    let conflicting_policy = policy_with(
        &ids,
        vec![
            base_grant(&ids),
            grant(
                11,
                permission_selector(ids.revision(), vec![permission(&ids)]),
                window(1, 91, 99),
                use_limit(Some(1)),
            ),
        ],
        Vec::new(),
        Vec::new(),
    );
    assert_denial(
        &evaluate(&conflicting_policy, &ids),
        AuthorizationDenialReason::EmptyConstraintIntersection,
    );
}

fn first_requirement() -> peritus_policy::ApprovalRequirement {
    approval_requirement(
        AuthorityTier::User,
        vec![ActorRole::Reviewer, ActorRole::HumanAuthority],
        vec![IndependenceRequirement::NotRequester],
        window(1, 0, 70),
    )
}

fn second_requirement() -> peritus_policy::ApprovalRequirement {
    approval_requirement(
        AuthorityTier::Organization,
        vec![ActorRole::HumanAuthority, ActorRole::DaemonService],
        vec![
            IndependenceRequirement::NotActionActor,
            IndependenceRequirement::NoReviewParticipation,
        ],
        window(1, 20, 80),
    )
}

#[test]
fn matching_approval_rules_form_the_exact_conjunction() {
    let ids = FixtureIds::new();
    let layers = vec![
        layer(
            PolicyTier::User,
            vec![approval_rule(20, any_selector(ids.revision()), first_requirement())],
        ),
        layer(
            PolicyTier::Project,
            vec![approval_rule(21, any_selector(ids.revision()), second_requirement())],
        ),
    ];
    let policy = policy_with(&ids, vec![base_grant(&ids)], Vec::new(), layers);
    let decision = evaluate(&policy, &ids);
    assert_eq!(decision.kind(), PolicyDecisionKind::ApprovalRequired);
    let challenge = decision.escalation_challenge().expect("challenge");
    assert_eq!(challenge.scope().validity(), window(1, 5, 90));
    assert_eq!(challenge.scope().use_limit().remaining(), Some(8));
    let requirement = challenge.requirement();
    assert_eq!(requirement.minimum_tier(), AuthorityTier::Organization);
    assert_eq!(requirement.approver_roles(), &[ActorRole::HumanAuthority]);
    assert_eq!(
        requirement.independence().as_slice(),
        &[
            IndependenceRequirement::NotRequester,
            IndependenceRequirement::NotActionActor,
            IndependenceRequirement::NoReviewParticipation,
        ]
    );
    assert_eq!(requirement.validity(), window(1, 20, 70));
}

#[test]
fn approval_role_and_validity_conflicts_deny_instead_of_looping() {
    let ids = FixtureIds::new();
    let disjoint_roles = approval_requirement(
        AuthorityTier::Project,
        vec![ActorRole::Reviewer],
        Vec::new(),
        window(1, 0, 100),
    );
    let policy = policy_with(
        &ids,
        vec![base_grant(&ids)],
        Vec::new(),
        vec![layer(
            PolicyTier::User,
            vec![
                approval_rule(20, any_selector(ids.revision()), disjoint_roles),
                approval_rule(21, any_selector(ids.revision()), second_requirement()),
            ],
        )],
    );
    assert_denial(&evaluate(&policy, &ids), AuthorizationDenialReason::ApprovalConstraintConflict);

    let validity_policy = policy_with(
        &ids,
        vec![base_grant(&ids)],
        Vec::new(),
        vec![layer(
            PolicyTier::User,
            vec![approval_rule(
                20,
                any_selector(ids.revision()),
                approval_requirement(
                    AuthorityTier::Project,
                    vec![ActorRole::HumanAuthority],
                    Vec::new(),
                    window(1, 91, 99),
                ),
            )],
        )],
    );
    assert_denial(
        &evaluate(&validity_policy, &ids),
        AuthorizationDenialReason::ApprovalConstraintConflict,
    );
}

#[test]
fn deny_dominates_approval_and_grants_in_every_rule_order() {
    let ids = FixtureIds::new();
    for deny_first in [false, true] {
        let (deny_digest, approval_digest) = if deny_first { (20, 21) } else { (21, 20) };
        let mut rules = vec![
            deny_rule(deny_digest, any_selector(ids.revision())),
            approval_rule(approval_digest, any_selector(ids.revision()), first_requirement()),
        ];
        rules.sort_by_key(|rule| rule.digest().into_bytes());
        let policy = policy_with(
            &ids,
            vec![base_grant(&ids)],
            Vec::new(),
            vec![layer(PolicyTier::User, rules)],
        );
        assert_denial(&evaluate(&policy, &ids), AuthorizationDenialReason::RestrictionDeny);
    }
    let immutable_policy = policy_with(
        &ids,
        vec![base_grant(&ids)],
        vec![deny_rule(12, any_selector(ids.revision()))],
        vec![layer(
            PolicyTier::User,
            vec![approval_rule(20, any_selector(ids.revision()), first_requirement())],
        )],
    );
    assert_denial(&evaluate(&immutable_policy, &ids), AuthorizationDenialReason::ImmutableDeny);
}

#[test]
fn empty_layer_and_nonmatching_rule_are_neutral() {
    let ids = FixtureIds::new();
    let nonmatching = ScopeSelector::new(
        ActorSelector::exact(vec![ids.other_actor]).expect("actor selector"),
        RoleSelector::any_within_parent(),
        EnvironmentSelector::any_within_parent(),
        PermissionSelector::any_within_parent(),
        ids.revision(),
    );
    let policy = policy_with(
        &ids,
        vec![base_grant(&ids)],
        Vec::new(),
        vec![
            layer(PolicyTier::System, Vec::new()),
            layer(PolicyTier::User, vec![approval_rule(20, nonmatching, first_requirement())]),
        ],
    );
    assert_eq!(evaluate(&policy, &ids).kind(), PolicyDecisionKind::Authorized);
}

#[test]
fn contradictory_grant_epochs_are_a_total_semantic_denial() {
    let ids = FixtureIds::new();
    let policy = policy_with(
        &ids,
        vec![
            base_grant(&ids),
            grant(
                11,
                permission_selector(ids.revision(), vec![permission(&ids)]),
                window(2, 5, 90),
                use_limit(Some(8)),
            ),
        ],
        Vec::new(),
        Vec::new(),
    );
    let outcome = policy.evaluate(
        request(&ids),
        AuthorityTimeState::new(crate::support::instant(1, 0)),
        crate::support::instant(1, 30),
    );
    let decision = outcome.expect("policy epoch contradiction is semantic, not authority time");
    assert_denial(&decision, AuthorizationDenialReason::EmptyConstraintIntersection);
    let denial = decision.denial().expect("whole-request denial");
    assert_eq!(denial.scope().actor_id(), ids.actor);
    assert_eq!(denial.scope().permissions().as_slice().len(), 1);
    assert_eq!(denial.evaluated_at(), crate::support::instant(1, 30));
}

#[test]
fn cross_epoch_approval_requirement_conflicts_with_effective_constraints() {
    let ids = FixtureIds::new();
    let requirement = approval_requirement(
        AuthorityTier::Project,
        vec![ActorRole::HumanAuthority],
        vec![IndependenceRequirement::NotRequester],
        window(2, 0, 70),
    );
    let policy = policy_with(
        &ids,
        vec![base_grant(&ids)],
        Vec::new(),
        vec![layer(
            PolicyTier::Project,
            vec![approval_rule(20, any_selector(ids.revision()), requirement)],
        )],
    );
    let outcome = policy.evaluate(
        request(&ids),
        AuthorityTimeState::new(crate::support::instant(1, 0)),
        crate::support::instant(1, 30),
    );
    let decision = outcome.expect("approval epoch contradiction is semantic, not authority time");
    assert_denial(&decision, AuthorizationDenialReason::ApprovalConstraintConflict);
    assert!(decision.escalation_challenge().is_none());
}
