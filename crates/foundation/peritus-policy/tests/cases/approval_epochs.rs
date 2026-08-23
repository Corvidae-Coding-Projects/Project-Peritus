use crate::support::{
    FixtureIds, PermissionSpec, PolicyInput, ScopeInput, any_selector, approval_requirement,
    approval_rule, descriptor, grant, layer, permission_selector, policy, registry, scope,
    use_limit, window,
};
use peritus_policy::{
    ActorRole, AuthorityTier, AuthorityTimeState, AuthorizationRequest, IndependenceRequirement,
    OperationClass, PolicyTier,
};

#[test]
fn approval_challenge_preserves_both_nondefault_validity_epochs() {
    let ids = FixtureIds::new();
    let permission = PermissionSpec { resource: ids.first_resource, name: "workspace.inspect" };
    let revision = ids.revision();
    let definition = policy(PolicyInput {
        actors: vec![ids.actor],
        roles: vec![ActorRole::Writer],
        environments: vec![ids.environment],
        permissions: vec![permission],
        revision,
        validity: window(7, 0, 100),
        uses: use_limit(Some(10)),
        grants: vec![grant(
            10,
            permission_selector(revision, vec![permission]),
            window(7, 5, 90),
            use_limit(Some(8)),
        )],
        immutable_denies: Vec::new(),
        operations: registry(vec![descriptor("workspace.inspect", OperationClass::Inspection)]),
        layers: vec![layer(
            PolicyTier::User,
            vec![approval_rule(
                20,
                any_selector(revision),
                approval_requirement(
                    AuthorityTier::User,
                    vec![ActorRole::HumanAuthority],
                    vec![IndependenceRequirement::NotRequester],
                    window(7, 20, 70),
                ),
            )],
        )],
    });
    let requested = scope(ScopeInput {
        actor: ids.actor,
        role: ActorRole::Writer,
        environment: ids.environment,
        permissions: vec![permission],
        revision,
        validity: window(7, 0, 100),
        uses: use_limit(Some(9)),
    });
    let decision = definition
        .evaluate(
            AuthorizationRequest::new(requested),
            AuthorityTimeState::new(crate::support::instant(7, 0)),
            crate::support::instant(7, 30),
        )
        .expect("same-epoch evaluation");
    let challenge = decision.escalation_challenge().expect("approval challenge");
    assert_eq!(challenge.scope().validity(), window(7, 5, 90));
    assert_eq!(challenge.requirement().validity(), window(7, 20, 70));
    assert_eq!(challenge.requirement().validity().not_before().epoch().get(), 7);
    assert_eq!(challenge.requirement().validity().expires_at().epoch().get(), 7);
}
