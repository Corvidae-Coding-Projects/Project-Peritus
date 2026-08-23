use crate::support::{
    FixtureIds, ModelApproval, ModelGate, ModelGrant, ModelInput, ModelRequest, ModelRule,
    ModelWindow, PermissionSpec, PolicyInput, ScopeInput, any_selector, approval_requirement,
    approval_rule, assert_generated_decision_exact, deny_rule, descriptor, evaluate_model, grant,
    layer, permission_selector, policy, registry, scope, use_limit, window,
};
use peritus_policy::{
    ActorRole, AuthorityTier, AuthorityTimeState, AuthorizationRequest, PolicyDecisionKind,
    PolicyTier,
};

const TRACE_SEEDS: [u64; 4] =
    [0x51a7_e5af_0011_0001, 0xa11c_e5ed_0022_0002, 0xd3f4_0170_0033_0003, 0xf00d_cafe_0044_0004];
const CASES_PER_SEED: usize = 64;

#[derive(Clone, Copy, Debug)]
struct Scenario {
    gates: u8,
    constraint_mode: u8,
    rule_mode: u8,
}

struct Generator(u64);

impl Generator {
    const fn new(seed: u64) -> Self {
        Self(seed)
    }

    const fn next(&mut self) -> u64 {
        self.0 =
            self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        self.0
    }

    const fn scenario(&mut self) -> Scenario {
        let bits = self.next();
        Scenario {
            gates: (bits & 0x1f) as u8,
            constraint_mode: ((bits >> 5) % 3) as u8,
            rule_mode: ((bits >> 7) % 5) as u8,
        }
    }
}

impl Scenario {
    const POLICY_MATCHES: u8 = 1;
    const BOUNDARY_CONTAINS: u8 = 2;
    const OPERATION_ALLOWED: u8 = 4;
    const IMMUTABLE_DENY: u8 = 8;
    const COVERAGE: u8 = 16;

    const fn has(self, gate: u8) -> bool {
        self.gates & gate != 0
    }

    const fn model_gate(self, gate: u8) -> ModelGate {
        if self.has(gate) { ModelGate::Satisfied } else { ModelGate::Rejected }
    }

    const fn immutable_gate(self) -> ModelGate {
        if self.has(Self::IMMUTABLE_DENY) { ModelGate::Rejected } else { ModelGate::Satisfied }
    }
}

fn model_grants(scenario: Scenario) -> Vec<ModelGrant> {
    if !scenario.has(Scenario::COVERAGE) {
        return Vec::new();
    }
    match scenario.constraint_mode {
        0 => vec![ModelGrant {
            covers: vec![true],
            applicable: true,
            validity: ModelWindow { start: 0, end: 100 },
            uses: Some(8),
        }],
        1 => vec![
            ModelGrant {
                covers: vec![true],
                applicable: true,
                validity: ModelWindow { start: 0, end: 90 },
                uses: Some(8),
            },
            ModelGrant {
                covers: vec![true],
                applicable: true,
                validity: ModelWindow { start: 20, end: 80 },
                uses: Some(4),
            },
        ],
        _ => vec![
            ModelGrant {
                covers: vec![true],
                applicable: true,
                validity: ModelWindow { start: 0, end: 40 },
                uses: Some(8),
            },
            ModelGrant {
                covers: vec![true],
                applicable: true,
                validity: ModelWindow { start: 60, end: 100 },
                uses: Some(4),
            },
        ],
    }
}

const fn model_approval(role_bits: u16, validity: ModelWindow) -> ModelApproval {
    ModelApproval { minimum_tier: 1, role_bits, independence_bits: 0, validity }
}

fn model_rules(mode: u8) -> Vec<ModelRule> {
    match mode {
        0 => Vec::new(),
        1 => vec![ModelRule::Deny],
        2 => vec![ModelRule::Approval(model_approval(1 << 7, ModelWindow { start: 10, end: 90 }))],
        3 => vec![
            ModelRule::Approval(model_approval(1 << 2, ModelWindow { start: 0, end: 100 })),
            ModelRule::Approval(model_approval(1 << 7, ModelWindow { start: 0, end: 100 })),
        ],
        _ => vec![
            ModelRule::Approval(model_approval(1 << 7, ModelWindow { start: 0, end: 40 })),
            ModelRule::Approval(model_approval(1 << 7, ModelWindow { start: 60, end: 100 })),
        ],
    }
}

fn model_input(scenario: Scenario) -> ModelInput {
    ModelInput {
        policy: scenario.model_gate(Scenario::POLICY_MATCHES),
        boundary: scenario.model_gate(Scenario::BOUNDARY_CONTAINS),
        operation: scenario.model_gate(Scenario::OPERATION_ALLOWED),
        immutable: scenario.immutable_gate(),
        boundary_validity: ModelWindow { start: 0, end: 100 },
        boundary_uses: Some(10),
        request: ModelRequest {
            pair_count: 1,
            validity: ModelWindow { start: 0, end: 100 },
            uses: Some(9),
            observed_tick: 50,
        },
        grants: model_grants(scenario),
        rules: model_rules(scenario.rule_mode),
    }
}

fn actual_grants(
    ids: &FixtureIds,
    permission: PermissionSpec,
    scenario: Scenario,
) -> Vec<peritus_policy::CeilingGrant> {
    model_grants(scenario)
        .into_iter()
        .enumerate()
        .map(|(index, grant_model)| {
            grant(
                10 + u8::try_from(index).expect("small grant index"),
                permission_selector(ids.revision(), vec![permission]),
                window(1, grant_model.validity.start, grant_model.validity.end),
                use_limit(grant_model.uses),
            )
        })
        .collect()
}

fn actual_requirement(
    roles: Vec<ActorRole>,
    validity: ModelWindow,
) -> peritus_policy::ApprovalRequirement {
    approval_requirement(
        AuthorityTier::User,
        roles,
        Vec::new(),
        window(1, validity.start, validity.end),
    )
}

fn actual_layers(ids: &FixtureIds, mode: u8) -> Vec<peritus_policy::RestrictionLayer> {
    let rules = match mode {
        0 => Vec::new(),
        1 => vec![deny_rule(20, any_selector(ids.revision()))],
        2 => vec![approval_rule(
            20,
            any_selector(ids.revision()),
            actual_requirement(vec![ActorRole::HumanAuthority], ModelWindow { start: 10, end: 90 }),
        )],
        3 => vec![
            approval_rule(
                20,
                any_selector(ids.revision()),
                actual_requirement(vec![ActorRole::Reviewer], ModelWindow { start: 0, end: 100 }),
            ),
            approval_rule(
                21,
                any_selector(ids.revision()),
                actual_requirement(
                    vec![ActorRole::HumanAuthority],
                    ModelWindow { start: 0, end: 100 },
                ),
            ),
        ],
        _ => vec![
            approval_rule(
                20,
                any_selector(ids.revision()),
                actual_requirement(
                    vec![ActorRole::HumanAuthority],
                    ModelWindow { start: 0, end: 40 },
                ),
            ),
            approval_rule(
                21,
                any_selector(ids.revision()),
                actual_requirement(
                    vec![ActorRole::HumanAuthority],
                    ModelWindow { start: 60, end: 100 },
                ),
            ),
        ],
    };
    if rules.is_empty() { Vec::new() } else { vec![layer(PolicyTier::User, rules)] }
}

fn evaluate_actual(ids: &FixtureIds, scenario: Scenario) -> peritus_policy::PolicyDecision {
    let permission = PermissionSpec { resource: ids.first_resource, name: "workspace.inspect" };
    let policy = policy(PolicyInput {
        actors: vec![ids.actor],
        roles: vec![ActorRole::Writer],
        environments: vec![ids.environment],
        permissions: vec![permission],
        revision: ids.revision(),
        validity: window(1, 0, 100),
        uses: use_limit(Some(10)),
        grants: actual_grants(ids, permission, scenario),
        immutable_denies: if scenario.has(Scenario::IMMUTABLE_DENY) {
            vec![deny_rule(12, any_selector(ids.revision()))]
        } else {
            Vec::new()
        },
        operations: if scenario.has(Scenario::OPERATION_ALLOWED) {
            registry(vec![descriptor(
                "workspace.inspect",
                peritus_policy::OperationClass::Inspection,
            )])
        } else {
            registry(Vec::new())
        },
        layers: actual_layers(ids, scenario.rule_mode),
    });
    let request_revision = requested_revision(ids, scenario);
    policy
        .evaluate(
            AuthorizationRequest::new(scope(ScopeInput {
                actor: requested_actor(ids, scenario),
                role: ActorRole::Writer,
                environment: ids.environment,
                permissions: vec![permission],
                revision: request_revision,
                validity: window(1, 0, 100),
                uses: use_limit(Some(9)),
            })),
            AuthorityTimeState::new(crate::support::instant(1, 0)),
            crate::support::instant(1, 50),
        )
        .expect("generated evaluation")
}

const fn requested_actor(ids: &FixtureIds, scenario: Scenario) -> peritus_types::ActorId {
    if scenario.has(Scenario::BOUNDARY_CONTAINS) { ids.actor } else { ids.other_actor }
}

const fn requested_revision(ids: &FixtureIds, scenario: Scenario) -> peritus_types::RevisionTuple {
    if scenario.has(Scenario::POLICY_MATCHES) {
        ids.revision()
    } else {
        peritus_types::RevisionTuple::new(
            ids.revision().acceptance_spec_id(),
            ids.revision().harness_id(),
            ids.revision().workspace_id(),
            ids.revision().workspace_generation(),
            ids.revision().workspace_revision(),
            ids.other_policy,
            ids.revision().provider_profile_id(),
        )
    }
}

#[test]
fn persisted_seed_cases_refine_the_independent_evaluator() {
    for seed in TRACE_SEEDS {
        let mut generator = Generator::new(seed);
        for case in 0..CASES_PER_SEED {
            let scenario = generator.scenario();
            let expected = evaluate_model(&model_input(scenario));
            let ids = FixtureIds::new();
            let actual = evaluate_actual(&ids, scenario);
            assert_generated_decision_exact(
                &actual,
                expected,
                requested_actor(&ids, scenario),
                ids.environment,
                requested_revision(&ids, scenario),
                PermissionSpec { resource: ids.first_resource, name: "workspace.inspect" },
                seed,
                case,
            );
        }
    }
}

#[test]
fn exhaustive_bounded_composition_never_increases_authority() {
    let ids = FixtureIds::new();
    for immutable_deny in [false, true] {
        for coverage in [false, true] {
            for rule_mode in 0..=4 {
                let base = Scenario {
                    gates: Scenario::POLICY_MATCHES
                        | Scenario::BOUNDARY_CONTAINS
                        | Scenario::OPERATION_ALLOWED
                        | if immutable_deny { Scenario::IMMUTABLE_DENY } else { 0 }
                        | if coverage { Scenario::COVERAGE } else { 0 },
                    constraint_mode: 0,
                    rule_mode: 0,
                };
                let restricted = Scenario { rule_mode, ..base };
                let base_rank = decision_rank(evaluate_actual(&ids, base).kind());
                let restricted_rank = decision_rank(evaluate_actual(&ids, restricted).kind());
                assert!(
                    restricted_rank <= base_rank,
                    "immutable {immutable_deny} coverage {coverage} rule {rule_mode}"
                );
            }
        }
    }
}

const fn decision_rank(kind: PolicyDecisionKind) -> u8 {
    match kind {
        PolicyDecisionKind::Denied => 0,
        PolicyDecisionKind::ApprovalRequired => 1,
        PolicyDecisionKind::Authorized => 2,
    }
}
