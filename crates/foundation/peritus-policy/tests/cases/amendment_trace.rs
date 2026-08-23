use super::amendment::{TIERS, assert_ceiling_preserved, assert_revision_rebound, tier_digest};
use crate::support::{
    FixtureIds, PermissionSpec, PolicyInput, approval_requirement, approval_rule, descriptor,
    digest, grant, layer, permission_set, policy, registry, use_limit, window,
};
use peritus_policy::{
    ActorRole, ActorSelector, AuthorityTier, EnvironmentSelector, IndependenceRequirement,
    PermissionSelector, PolicyAmendmentProposal, PolicyErrorKind, PolicyTier, RestrictionLayer,
    RestrictionRule, RoleSelector, ScopeSelector,
};

const SEEDS: [u64; 4] =
    [0xa6e1_d001_1000_0001, 0xa6e1_d001_2000_0002, 0xa6e1_d001_3000_0003, 0xa6e1_d001_4000_0004];
const CASES_PER_SEED: usize = 48;

struct Generator(u64);

impl Generator {
    const fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        self.0
    }
}

fn exact_selector(ids: &FixtureIds) -> ScopeSelector {
    ScopeSelector::new(
        ActorSelector::exact(vec![ids.actor]).expect("actor selector"),
        RoleSelector::exact(vec![ActorRole::Writer]).expect("role selector"),
        EnvironmentSelector::exact(vec![ids.environment]).expect("environment selector"),
        PermissionSelector::exact(permission_set(vec![PermissionSpec {
            resource: ids.first_resource,
            name: "workspace.inspect",
        }])),
        ids.revision(),
    )
}

fn approval_rule_for(ids: &FixtureIds, digest_byte: u8, replacement: bool) -> RestrictionRule {
    let requirement = if replacement {
        approval_requirement(
            AuthorityTier::Organization,
            vec![ActorRole::Reviewer, ActorRole::HumanAuthority],
            vec![
                IndependenceRequirement::NotRequester,
                IndependenceRequirement::NoReviewParticipation,
            ],
            window(1, 20, 70),
        )
    } else {
        approval_requirement(
            AuthorityTier::User,
            vec![ActorRole::HumanAuthority],
            vec![IndependenceRequirement::NotActionActor],
            window(1, 10, 80),
        )
    };
    approval_rule(digest_byte, exact_selector(ids), requirement)
}

fn rich_base_policy(
    ids: &FixtureIds,
    omitted: Option<PolicyTier>,
) -> peritus_policy::PolicyDefinition {
    let revision = ids.revision();
    let permission = PermissionSpec { resource: ids.first_resource, name: "workspace.inspect" };
    let layers = TIERS
        .into_iter()
        .filter(|tier| Some(*tier) != omitted)
        .map(|tier| layer(tier, vec![approval_rule_for(ids, tier_digest(tier), false)]))
        .collect();
    policy(PolicyInput {
        actors: vec![ids.actor],
        roles: vec![ActorRole::Writer],
        environments: vec![ids.environment],
        permissions: vec![permission],
        revision,
        validity: window(1, 0, 100),
        uses: use_limit(Some(9)),
        grants: vec![grant(10, exact_selector(ids), window(1, 5, 90), use_limit(Some(8)))],
        immutable_denies: Vec::new(),
        operations: registry(vec![descriptor(
            "workspace.inspect",
            peritus_policy::OperationClass::Inspection,
        )]),
        layers,
    })
}

fn assert_rule_exact(
    actual: &RestrictionRule,
    expected: &RestrictionRule,
    successor_policy: peritus_types::PolicyId,
    diagnostic: &str,
) {
    assert_eq!(actual.digest(), expected.digest(), "{diagnostic}: digest");
    assert_eq!(actual.selector().actors(), expected.selector().actors(), "{diagnostic}: actors");
    assert_eq!(actual.selector().roles(), expected.selector().roles(), "{diagnostic}: roles");
    assert_eq!(
        actual.selector().environments(),
        expected.selector().environments(),
        "{diagnostic}: environments"
    );
    assert_eq!(
        actual.selector().permissions(),
        expected.selector().permissions(),
        "{diagnostic}: permissions"
    );
    assert_revision_rebound(
        *expected.selector().revision(),
        *actual.selector().revision(),
        successor_policy,
    );
    assert_eq!(actual.is_deny(), expected.is_deny(), "{diagnostic}: rule kind");
    match (actual.approval_requirement(), expected.approval_requirement()) {
        (Some(actual), Some(expected)) => {
            assert_eq!(actual.minimum_tier(), expected.minimum_tier(), "{diagnostic}: min tier");
            assert_eq!(
                actual.approver_roles(),
                expected.approver_roles(),
                "{diagnostic}: approver roles"
            );
            assert_eq!(
                actual.independence(),
                expected.independence(),
                "{diagnostic}: independence"
            );
            assert_eq!(actual.validity(), expected.validity(), "{diagnostic}: validity");
        }
        (None, None) => {}
        _ => panic!("{diagnostic}: approval requirement kind"),
    }
}

fn assert_candidate_exact(
    base: &peritus_policy::PolicyDefinition,
    candidate: &peritus_policy::PolicyRevisionCandidate,
    target: PolicyTier,
    replacement: &RestrictionRule,
    inserted: bool,
    ids: &FixtureIds,
    diagnostic: &str,
) {
    assert_eq!(candidate.base_policy_id(), ids.policy, "{diagnostic}: base id");
    assert_eq!(candidate.successor_policy_id(), ids.other_policy, "{diagnostic}: successor id");
    assert_eq!(candidate.tier(), target, "{diagnostic}: tier");
    assert_eq!(candidate.amendment_digest(), digest(90), "{diagnostic}: digest");
    let successor = candidate.successor_policy();
    assert_eq!(successor.policy_id(), ids.other_policy, "{diagnostic}: policy id");
    assert_eq!(successor.operations(), base.operations(), "{diagnostic}: operations");
    assert_ceiling_preserved(base.ceiling(), successor.ceiling(), ids.other_policy);
    assert_eq!(successor.layers().len(), TIERS.len(), "{diagnostic}: layer count");
    assert_eq!(base.layers().len() + usize::from(inserted), successor.layers().len());
    for (index, tier) in TIERS.into_iter().enumerate() {
        let actual = &successor.layers()[index];
        assert_eq!(actual.tier(), tier, "{diagnostic}: layer {index} tier");
        assert_eq!(actual.rules().len(), 1, "{diagnostic}: layer {index} rule count");
        let expected = if tier == target {
            replacement
        } else {
            let base_layer =
                base.layers().iter().find(|layer| layer.tier() == tier).expect("base tier");
            &base_layer.rules()[0]
        };
        assert_rule_exact(
            &actual.rules()[0],
            expected,
            ids.other_policy,
            &format!("{diagnostic}: layer {index}"),
        );
    }
}

#[test]
fn generated_amendment_and_rebind_results_match_independent_oracle() {
    for seed in SEEDS {
        let mut generator = Generator(seed);
        for case in 0..CASES_PER_SEED {
            let bits = generator.next();
            let mode = (bits % 6) as u8;
            let target = TIERS[((bits >> 8) as usize) % TIERS.len()];
            let diagnostic = format!("seed {seed:#x} case {case} mode {mode}");
            let inserted = mode == 1;
            let ids = FixtureIds::new();
            let base = rich_base_policy(&ids, inserted.then_some(target));
            let replacement_digest = tier_digest(target).saturating_add(60);

            if mode == 2 {
                let error = PolicyAmendmentProposal::new(
                    ids.policy,
                    ids.policy,
                    target,
                    layer(target, Vec::new()),
                    digest(90),
                )
                .expect_err(&diagnostic);
                assert_eq!(error.kind(), PolicyErrorKind::AmendmentPolicyIdReuse, "{diagnostic}");
                continue;
            }
            if mode == 3 {
                let other_tier =
                    TIERS[(TIERS.iter().position(|value| *value == target).expect("known tier")
                        + 1)
                        % TIERS.len()];
                let error = PolicyAmendmentProposal::new(
                    ids.policy,
                    ids.other_policy,
                    target,
                    RestrictionLayer::new(other_tier, Vec::new()).expect("layer"),
                    digest(90),
                )
                .expect_err(&diagnostic);
                assert_eq!(error.kind(), PolicyErrorKind::AmendmentTierMismatch, "{diagnostic}");
                continue;
            }

            let proposal_base = if mode == 4 { ids.other_policy } else { ids.policy };
            let replacement_selector = if mode == 5 {
                ScopeSelector::new(
                    ActorSelector::exact(vec![ids.other_actor]).expect("actor selector"),
                    RoleSelector::any_within_parent(),
                    EnvironmentSelector::any_within_parent(),
                    PermissionSelector::any_within_parent(),
                    ids.revision(),
                )
            } else {
                exact_selector(&ids)
            };
            let replacement = approval_rule(
                replacement_digest,
                replacement_selector,
                approval_requirement(
                    AuthorityTier::Organization,
                    vec![ActorRole::Reviewer, ActorRole::HumanAuthority],
                    vec![
                        IndependenceRequirement::NotRequester,
                        IndependenceRequirement::NoReviewParticipation,
                    ],
                    window(1, 20, 70),
                ),
            );
            let proposal = PolicyAmendmentProposal::new(
                proposal_base,
                if mode == 4 { ids.policy } else { ids.other_policy },
                target,
                layer(target, vec![replacement]),
                digest(90),
            )
            .expect("valid proposal shape");
            if mode == 4 {
                let error = base.preview_amendment(&proposal).expect_err(&diagnostic);
                assert_eq!(error.kind(), PolicyErrorKind::AmendmentBaseMismatch, "{diagnostic}");
            } else if mode == 5 {
                let error = base.preview_amendment(&proposal).expect_err(&diagnostic);
                assert_eq!(error.kind(), PolicyErrorKind::SelectorOutsideBoundary, "{diagnostic}");
            } else {
                let candidate = base.preview_amendment(&proposal).expect(&diagnostic);
                let expected_replacement = approval_rule_for(&ids, replacement_digest, true);
                assert_candidate_exact(
                    &base,
                    &candidate,
                    target,
                    &expected_replacement,
                    inserted,
                    &ids,
                    &diagnostic,
                );
            }
        }
    }
}
