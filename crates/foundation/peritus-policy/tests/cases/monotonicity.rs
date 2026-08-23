use crate::support::{
    FixtureIds, PermissionSpec, PolicyInput, ScopeInput, any_selector, approval_requirement,
    approval_rule, deny_rule, descriptor, grant, layer, permission_selector, policy, registry,
    scope, use_limit, window,
};
use peritus_policy::{
    ActorRole, ActorSelector, AuthorityTier, AuthorityTimeState, EnvironmentSelector,
    OperationClass, PermissionSelector, PolicyDecisionKind, PolicyTier, RestrictionLayer,
    RoleSelector, ScopeSelector,
};

const BASE_MODES: usize = 4;
const ADDED_MODES: usize = 5;
const QUERY_CASES: usize = 8;

const fn inspected(ids: &FixtureIds) -> PermissionSpec {
    PermissionSpec { resource: ids.first_resource, name: "workspace.inspect" }
}

fn actor_selector(ids: &FixtureIds, actor: peritus_types::ActorId) -> ScopeSelector {
    ScopeSelector::new(
        ActorSelector::exact(vec![actor]).expect("canonical actor selector"),
        RoleSelector::any_within_parent(),
        EnvironmentSelector::any_within_parent(),
        PermissionSelector::any_within_parent(),
        ids.revision(),
    )
}

fn approval(
    ids: &FixtureIds,
    digest: u8,
    roles: Vec<ActorRole>,
) -> peritus_policy::RestrictionRule {
    approval_rule(
        digest,
        any_selector(ids.revision()),
        approval_requirement(AuthorityTier::User, roles, Vec::new(), window(1, 10, 100)),
    )
}

fn base_layers(ids: &FixtureIds, mode: usize) -> Vec<RestrictionLayer> {
    match mode {
        0 => Vec::new(),
        1 => {
            vec![layer(PolicyTier::User, vec![approval(ids, 20, vec![ActorRole::HumanAuthority])])]
        }
        2 => {
            vec![layer(PolicyTier::User, vec![deny_rule(20, actor_selector(ids, ids.other_actor))])]
        }
        _ => vec![layer(PolicyTier::User, vec![deny_rule(20, any_selector(ids.revision()))])],
    }
}

fn added_layer(ids: &FixtureIds, mode: usize) -> RestrictionLayer {
    let rules = match mode {
        0 => Vec::new(),
        1 => vec![deny_rule(30, any_selector(ids.revision()))],
        2 => vec![approval(ids, 30, vec![ActorRole::HumanAuthority])],
        3 => vec![deny_rule(30, actor_selector(ids, ids.other_actor))],
        _ => vec![
            approval(ids, 30, vec![ActorRole::Reviewer]),
            approval(ids, 31, vec![ActorRole::HumanAuthority]),
        ],
    };
    layer(PolicyTier::Project, rules)
}

fn evaluate(
    ids: &FixtureIds,
    base_mode: usize,
    added_mode: Option<usize>,
    query_case: usize,
) -> PolicyDecisionKind {
    let permission = inspected(ids);
    let validity = window(1, 10, 100);
    let mut layers = base_layers(ids, base_mode);
    if let Some(mode) = added_mode {
        layers.push(added_layer(ids, mode));
    }
    let definition = policy(PolicyInput {
        actors: vec![ids.actor, ids.other_actor],
        roles: vec![ActorRole::Writer],
        environments: vec![ids.environment],
        permissions: vec![permission],
        revision: ids.revision(),
        validity,
        uses: use_limit(Some(5)),
        grants: vec![grant(
            10,
            permission_selector(ids.revision(), vec![permission]),
            validity,
            use_limit(Some(5)),
        )],
        immutable_denies: Vec::new(),
        operations: registry(vec![descriptor("workspace.inspect", OperationClass::Inspection)]),
        layers,
    });
    let actor = if query_case == 1 { ids.other_actor } else { ids.actor };
    let role = if query_case == 2 { ActorRole::Reviewer } else { ActorRole::Writer };
    let environment = if query_case == 3 { ids.other_environment } else { ids.environment };
    let revision = if query_case == 4 {
        peritus_types::RevisionTuple::new(
            ids.revision().acceptance_spec_id(),
            ids.revision().harness_id(),
            ids.revision().workspace_id(),
            ids.revision().workspace_generation(),
            ids.revision().workspace_revision(),
            ids.other_policy,
            ids.revision().provider_profile_id(),
        )
    } else {
        ids.revision()
    };
    let requested_permission = if query_case == 5 {
        PermissionSpec { resource: ids.second_resource, name: "workspace.inspect" }
    } else {
        permission
    };
    let tick = match query_case {
        6 => 9,
        7 => 100,
        _ => 50,
    };
    definition
        .evaluate(
            peritus_policy::AuthorizationRequest::new(scope(ScopeInput {
                actor,
                role,
                environment,
                permissions: vec![requested_permission],
                revision,
                validity,
                uses: use_limit(Some(5)),
            })),
            AuthorityTimeState::new(crate::support::instant(1, 0)),
            crate::support::instant(1, tick),
        )
        .expect("bounded query uses one valid authority epoch")
        .kind()
}

const fn rank(kind: PolicyDecisionKind) -> u8 {
    match kind {
        PolicyDecisionKind::Denied => 0,
        PolicyDecisionKind::ApprovalRequired => 1,
        PolicyDecisionKind::Authorized => 2,
    }
}

#[test]
fn independent_bounded_query_domain_shrinks_after_exactly_one_appended_layer() {
    let ids = FixtureIds::new();
    for base_mode in 0..BASE_MODES {
        for added_mode in 0..ADDED_MODES {
            for query_case in 0..QUERY_CASES {
                let before = rank(evaluate(&ids, base_mode, None, query_case));
                let after = rank(evaluate(&ids, base_mode, Some(added_mode), query_case));
                assert!(
                    after <= before,
                    "base {base_mode} added {added_mode} query {query_case}: {before} -> {after}"
                );
                for minimum in 0..=2 {
                    assert!(
                        after < minimum || before >= minimum,
                        "set inclusion failed: base {base_mode} added {added_mode} query \
                         {query_case} threshold {minimum}"
                    );
                }
            }
        }
    }
}
