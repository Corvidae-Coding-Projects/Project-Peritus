use crate::support::{
    FixtureIds, PermissionSpec, PolicyInput, any_selector, deny_rule, descriptor, digest, grant,
    layer, permission_selector, policy, registry, use_limit, window,
};
use peritus_policy::{
    ActorRole, ActorSelector, EnvironmentSelector, PermissionSelector, PolicyAmendmentProposal,
    PolicyErrorKind, PolicyTier, RestrictionLayer, RoleSelector, ScopeSelector,
};

pub(super) const TIERS: [PolicyTier; 6] = [
    PolicyTier::System,
    PolicyTier::User,
    PolicyTier::Project,
    PolicyTier::Run,
    PolicyTier::Session,
    PolicyTier::RoleHarness,
];

pub(super) const fn tier_digest(tier: PolicyTier) -> u8 {
    match tier {
        PolicyTier::System => 20,
        PolicyTier::User => 21,
        PolicyTier::Project => 22,
        PolicyTier::Run => 23,
        PolicyTier::Session => 24,
        PolicyTier::RoleHarness => 25,
    }
}

pub(super) fn base_policy(
    ids: &FixtureIds,
    omitted: Option<PolicyTier>,
) -> peritus_policy::PolicyDefinition {
    let revision = ids.revision();
    let permission = PermissionSpec { resource: ids.first_resource, name: "workspace.inspect" };
    let layers = TIERS
        .into_iter()
        .filter(|tier| Some(*tier) != omitted)
        .map(|tier| layer(tier, vec![deny_rule(tier_digest(tier), any_selector(revision))]))
        .collect();
    policy(PolicyInput {
        actors: vec![ids.actor],
        roles: vec![ActorRole::Writer],
        environments: vec![ids.environment],
        permissions: vec![permission],
        revision,
        validity: window(1, 0, 100),
        uses: use_limit(Some(9)),
        grants: vec![grant(
            10,
            permission_selector(revision, vec![permission]),
            window(1, 5, 90),
            use_limit(Some(8)),
        )],
        immutable_denies: vec![deny_rule(11, any_selector(revision))],
        operations: registry(vec![descriptor(
            "workspace.inspect",
            peritus_policy::OperationClass::Inspection,
        )]),
        layers,
    })
}

pub(super) fn assert_revision_rebound(
    previous: peritus_types::RevisionTuple,
    successor: peritus_types::RevisionTuple,
    successor_policy: peritus_types::PolicyId,
) {
    assert_eq!(successor.acceptance_spec_id(), previous.acceptance_spec_id());
    assert_eq!(successor.harness_id(), previous.harness_id());
    assert_eq!(successor.workspace_id(), previous.workspace_id());
    assert_eq!(successor.workspace_generation(), previous.workspace_generation());
    assert_eq!(successor.workspace_revision(), previous.workspace_revision());
    assert_eq!(successor.provider_profile_id(), previous.provider_profile_id());
    assert_eq!(successor.policy_id(), successor_policy);
}

pub(super) fn assert_ceiling_preserved(
    previous: &peritus_policy::AuthorityCeiling,
    successor: &peritus_policy::AuthorityCeiling,
    successor_policy: peritus_types::PolicyId,
) {
    let previous_boundary = previous.boundary();
    let successor_boundary = successor.boundary();
    assert_eq!(successor_boundary.actors(), previous_boundary.actors());
    assert_eq!(successor_boundary.roles(), previous_boundary.roles());
    assert_eq!(successor_boundary.environments(), previous_boundary.environments());
    assert_eq!(successor_boundary.permissions(), previous_boundary.permissions());
    assert_eq!(successor_boundary.validity(), previous_boundary.validity());
    assert_eq!(successor_boundary.use_limit(), previous_boundary.use_limit());
    assert_revision_rebound(
        *previous_boundary.revision(),
        *successor_boundary.revision(),
        successor_policy,
    );
    assert_eq!(successor.grants().len(), previous.grants().len());
    for (before, after) in previous.grants().iter().zip(successor.grants()) {
        assert_eq!(after.digest(), before.digest());
        assert_eq!(after.validity(), before.validity());
        assert_eq!(after.use_limit(), before.use_limit());
        assert_eq!(after.selector().actors(), before.selector().actors());
        assert_eq!(after.selector().roles(), before.selector().roles());
        assert_eq!(after.selector().environments(), before.selector().environments());
        assert_eq!(after.selector().permissions(), before.selector().permissions());
        assert_revision_rebound(
            *before.selector().revision(),
            *after.selector().revision(),
            successor_policy,
        );
    }
    assert_eq!(successor.immutable_denies().len(), previous.immutable_denies().len());
    for (before, after) in previous.immutable_denies().iter().zip(successor.immutable_denies()) {
        assert_eq!(after.digest(), before.digest());
        assert_eq!(after.is_deny(), before.is_deny());
        assert_eq!(after.selector().actors(), before.selector().actors());
        assert_eq!(after.selector().roles(), before.selector().roles());
        assert_eq!(after.selector().environments(), before.selector().environments());
        assert_eq!(after.selector().permissions(), before.selector().permissions());
        assert_revision_rebound(
            *before.selector().revision(),
            *after.selector().revision(),
            successor_policy,
        );
    }
}

#[test]
fn every_tier_can_be_inserted_as_the_only_changed_layer() {
    let ids = FixtureIds::new();
    for target in TIERS {
        let base = base_policy(&ids, Some(target));
        let replacement_digest = tier_digest(target) + 40;
        let replacement =
            layer(target, vec![deny_rule(replacement_digest, any_selector(ids.revision()))]);
        let proposal = PolicyAmendmentProposal::new(
            ids.policy,
            ids.other_policy,
            target,
            replacement,
            digest(90),
        )
        .expect("proposal");
        let candidate = base.preview_amendment(&proposal).expect("candidate");
        assert_eq!(candidate.base_policy_id(), ids.policy);
        assert_eq!(candidate.successor_policy_id(), ids.other_policy);
        assert_eq!(candidate.tier(), target);
        assert_eq!(candidate.amendment_digest(), digest(90));

        let successor = candidate.successor_policy();
        assert_eq!(successor.operations(), base.operations());
        assert_ceiling_preserved(base.ceiling(), successor.ceiling(), ids.other_policy);
        assert_eq!(successor.layers().len(), TIERS.len());
        for (index, tier) in TIERS.into_iter().enumerate() {
            let successor_layer = &successor.layers()[index];
            assert_eq!(successor_layer.tier(), tier);
            let expected_digest =
                if tier == target { replacement_digest } else { tier_digest(tier) };
            assert_eq!(successor_layer.rules()[0].digest(), digest(expected_digest));
            assert_revision_rebound(
                ids.revision(),
                *successor_layer.rules()[0].selector().revision(),
                ids.other_policy,
            );
        }
    }
}

#[test]
fn every_existing_tier_is_replaced_exactly_once() {
    let ids = FixtureIds::new();
    for target in TIERS {
        let base = base_policy(&ids, None);
        let replacement_digest = tier_digest(target) + 50;
        let proposal = PolicyAmendmentProposal::new(
            ids.policy,
            ids.other_policy,
            target,
            layer(target, vec![deny_rule(replacement_digest, any_selector(ids.revision()))]),
            digest(91),
        )
        .expect("proposal");
        let candidate = base.preview_amendment(&proposal).expect("candidate");
        assert_eq!(candidate.successor_policy().layers().len(), TIERS.len());
        for successor_layer in candidate.successor_policy().layers() {
            let expected = if successor_layer.tier() == target {
                replacement_digest
            } else {
                tier_digest(successor_layer.tier())
            };
            assert_eq!(successor_layer.rules().len(), 1);
            assert_eq!(successor_layer.rules()[0].digest(), digest(expected));
        }
    }
}

#[test]
fn amendment_rejects_identity_tier_and_ceiling_violations() {
    let ids = FixtureIds::new();
    let wrong_tier = PolicyAmendmentProposal::new(
        ids.policy,
        ids.other_policy,
        PolicyTier::User,
        RestrictionLayer::new(PolicyTier::Project, Vec::new()).expect("layer"),
        digest(1),
    )
    .expect_err("tier mismatch");
    assert_eq!(wrong_tier.kind(), PolicyErrorKind::AmendmentTierMismatch);

    let reused = PolicyAmendmentProposal::new(
        ids.policy,
        ids.policy,
        PolicyTier::User,
        layer(PolicyTier::User, Vec::new()),
        digest(2),
    )
    .expect_err("identity reuse");
    assert_eq!(reused.kind(), PolicyErrorKind::AmendmentPolicyIdReuse);

    let base = base_policy(&ids, None);
    let wrong_base = PolicyAmendmentProposal::new(
        ids.other_policy,
        ids.policy,
        PolicyTier::User,
        layer(PolicyTier::User, Vec::new()),
        digest(3),
    )
    .expect("proposal");
    assert_eq!(
        base.preview_amendment(&wrong_base).expect_err("base mismatch").kind(),
        PolicyErrorKind::AmendmentBaseMismatch
    );

    let outside_selector = ScopeSelector::new(
        ActorSelector::exact(vec![ids.other_actor]).expect("actor selector"),
        RoleSelector::any_within_parent(),
        EnvironmentSelector::any_within_parent(),
        PermissionSelector::any_within_parent(),
        ids.revision(),
    );
    let outside = PolicyAmendmentProposal::new(
        ids.policy,
        ids.other_policy,
        PolicyTier::User,
        layer(PolicyTier::User, vec![deny_rule(99, outside_selector)]),
        digest(4),
    )
    .expect("proposal");
    assert_eq!(
        base.preview_amendment(&outside).expect_err("outside ceiling").kind(),
        PolicyErrorKind::SelectorOutsideBoundary
    );
}

#[test]
fn policy_definition_rejects_a_second_policy_identity() {
    let ids = FixtureIds::new();
    let boundary = peritus_policy::AuthorityBoundary::new(
        vec![ids.actor],
        vec![ActorRole::Writer],
        vec![ids.environment],
        crate::support::permission_set(vec![PermissionSpec {
            resource: ids.first_resource,
            name: "workspace.inspect",
        }]),
        ids.revision(),
        window(1, 0, 100),
        use_limit(Some(2)),
    )
    .expect("boundary");
    let ceiling =
        peritus_policy::AuthorityCeiling::new(boundary, Vec::new(), Vec::new()).expect("ceiling");
    let error = peritus_policy::PolicyDefinition::new(
        ids.other_policy,
        ceiling,
        registry(Vec::new()),
        Vec::new(),
    )
    .expect_err("policy identity mismatch");
    assert_eq!(error.kind(), PolicyErrorKind::PolicyRevisionMismatch);
}
