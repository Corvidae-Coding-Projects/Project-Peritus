use crate::support::{
    ModelApproval, ModelDecision, ModelDenial, ModelGate, ModelGrant, ModelInput, ModelRequest,
    ModelRule, ModelWindow, evaluate_model,
};

fn baseline() -> ModelInput {
    ModelInput {
        policy: ModelGate::Satisfied,
        boundary: ModelGate::Satisfied,
        operation: ModelGate::Satisfied,
        immutable: ModelGate::Satisfied,
        boundary_validity: ModelWindow { start: 0, end: 100 },
        boundary_uses: Some(8),
        request: ModelRequest {
            pair_count: 2,
            validity: ModelWindow { start: 5, end: 90 },
            uses: Some(7),
            observed_tick: 30,
        },
        grants: vec![
            ModelGrant {
                covers: vec![true, false],
                applicable: true,
                validity: ModelWindow { start: 10, end: 80 },
                uses: Some(6),
            },
            ModelGrant {
                covers: vec![false, true],
                applicable: true,
                validity: ModelWindow { start: 20, end: 70 },
                uses: Some(5),
            },
        ],
        rules: Vec::new(),
    }
}

#[test]
fn independent_model_uses_union_coverage_and_all_applicable_constraints() {
    assert_eq!(
        evaluate_model(&baseline()),
        ModelDecision::Authorized { validity: ModelWindow { start: 20, end: 70 }, uses: Some(5) }
    );
}

#[test]
fn independent_model_denies_incomplete_pairs_and_empty_intersections() {
    let mut incomplete = baseline();
    incomplete.grants.pop();
    assert_eq!(evaluate_model(&incomplete), ModelDecision::Denied(ModelDenial::IncompleteCoverage));

    let mut conflicting = baseline();
    conflicting.grants[1].validity = ModelWindow { start: 80, end: 90 };
    assert_eq!(
        evaluate_model(&conflicting),
        ModelDecision::Denied(ModelDenial::ConstraintConflict)
    );
}

#[test]
fn independent_model_conjoins_approval_and_deny_dominates() {
    let mut input = baseline();
    input.rules = vec![
        ModelRule::Approval(ModelApproval {
            minimum_tier: 0,
            role_bits: 0b0110,
            independence_bits: 0b0001,
            validity: ModelWindow { start: 5, end: 60 },
        }),
        ModelRule::Neutral,
        ModelRule::Approval(ModelApproval {
            minimum_tier: 2,
            role_bits: 0b0010,
            independence_bits: 0b0100,
            validity: ModelWindow { start: 15, end: 50 },
        }),
    ];
    assert_eq!(
        evaluate_model(&input),
        ModelDecision::ApprovalRequired {
            validity: ModelWindow { start: 20, end: 70 },
            uses: Some(5),
            requirement: ModelApproval {
                minimum_tier: 2,
                role_bits: 0b0010,
                independence_bits: 0b0101,
                validity: ModelWindow { start: 20, end: 50 },
            },
        }
    );
    input.rules.push(ModelRule::Deny);
    assert_eq!(evaluate_model(&input), ModelDecision::Denied(ModelDenial::Restriction));
}
