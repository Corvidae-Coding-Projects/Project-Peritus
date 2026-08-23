use crate::support::{
    FixtureIds, PermissionSpec, PolicyInput, ScopeInput, any_selector, approval_requirement,
    approval_rule, descriptor, grant, layer, mandatory_risk, permission_selector, policy, registry,
    scope, use_limit, window,
};
use peritus_policy::{
    ActorRole, AuthorityTier, AuthorityTimeState, IndependenceRequirement, OperationClass,
    OperationDescriptor, PolicyErrorKind, PolicyTier, RiskClass, RiskSet,
};
use peritus_types::CapabilityName;

const ROLES: [ActorRole; 11] = [
    ActorRole::Writer,
    ActorRole::Fixer,
    ActorRole::Reviewer,
    ActorRole::Evaluator,
    ActorRole::GateRunner,
    ActorRole::Orchestrator,
    ActorRole::EvolutionAgent,
    ActorRole::HumanAuthority,
    ActorRole::DaemonService,
    ActorRole::ProviderToolWorker,
    ActorRole::Plugin,
];

const OPERATIONS: [OperationClass; 14] = [
    OperationClass::Inspection,
    OperationClass::WorkspaceMutation,
    OperationClass::Execution,
    OperationClass::Network,
    OperationClass::DependencyEnvironment,
    OperationClass::RepositoryHistoryMutation,
    OperationClass::SecretUse,
    OperationClass::ExternalSideEffect,
    OperationClass::Acceptance,
    OperationClass::Waiver,
    OperationClass::PolicyAmendment,
    OperationClass::HarnessPromotion,
    OperationClass::HumanAuthority,
    OperationClass::RawEffect,
];

const fn expected(role: ActorRole, operation: OperationClass) -> bool {
    match role {
        ActorRole::Writer | ActorRole::Fixer => matches!(
            operation,
            OperationClass::Inspection
                | OperationClass::WorkspaceMutation
                | OperationClass::Execution
                | OperationClass::Network
                | OperationClass::DependencyEnvironment
                | OperationClass::RepositoryHistoryMutation
                | OperationClass::SecretUse
                | OperationClass::ExternalSideEffect
        ),
        ActorRole::Reviewer | ActorRole::Plugin => {
            matches!(operation, OperationClass::Inspection)
        }
        ActorRole::Evaluator | ActorRole::GateRunner => {
            matches!(operation, OperationClass::Inspection | OperationClass::Execution)
        }
        ActorRole::Orchestrator => matches!(
            operation,
            OperationClass::Inspection
                | OperationClass::Execution
                | OperationClass::Acceptance
                | OperationClass::PolicyAmendment
                | OperationClass::HarnessPromotion
        ),
        ActorRole::EvolutionAgent => matches!(
            operation,
            OperationClass::Inspection
                | OperationClass::WorkspaceMutation
                | OperationClass::Execution
                | OperationClass::Network
                | OperationClass::DependencyEnvironment
        ),
        ActorRole::HumanAuthority => matches!(
            operation,
            OperationClass::Inspection
                | OperationClass::Acceptance
                | OperationClass::Waiver
                | OperationClass::PolicyAmendment
                | OperationClass::HarnessPromotion
                | OperationClass::HumanAuthority
        ),
        ActorRole::DaemonService => matches!(
            operation,
            OperationClass::Inspection
                | OperationClass::Execution
                | OperationClass::Network
                | OperationClass::SecretUse
                | OperationClass::ExternalSideEffect
                | OperationClass::Acceptance
                | OperationClass::PolicyAmendment
                | OperationClass::HarnessPromotion
        ),
        ActorRole::ProviderToolWorker => matches!(
            operation,
            OperationClass::Inspection
                | OperationClass::WorkspaceMutation
                | OperationClass::Execution
                | OperationClass::Network
                | OperationClass::DependencyEnvironment
                | OperationClass::RepositoryHistoryMutation
                | OperationClass::SecretUse
                | OperationClass::ExternalSideEffect
                | OperationClass::RawEffect
        ),
    }
}

#[test]
fn complete_role_operation_matrix_matches_compiled_separation() {
    for role in ROLES {
        for operation in OPERATIONS {
            assert_eq!(
                role.permits_operation(operation),
                expected(role, operation),
                "role {role:?}, operation {operation:?}"
            );
        }
    }
}

#[test]
fn every_operation_class_requires_its_non_downgradable_risk() {
    for operation in OPERATIONS {
        let wrong_risk = if mandatory_risk(operation) == RiskClass::Read {
            RiskClass::ScopedWrite
        } else {
            RiskClass::Read
        };
        let error = OperationDescriptor::new(
            CapabilityName::new("operation.test".to_owned()).expect("operation name"),
            operation,
            RiskSet::new(vec![wrong_risk]).expect("risk set"),
        )
        .expect_err("mandatory risk omission must fail");
        assert_eq!(error.kind(), PolicyErrorKind::InvalidOperationRisk, "{operation:?}");
    }
}

#[test]
fn registry_lookup_is_exact_and_unknown_names_never_authorize() {
    let registry = registry(vec![
        descriptor("process.execute", OperationClass::Execution),
        descriptor("workspace.inspect", OperationClass::Inspection),
    ]);
    let inspection = CapabilityName::new("workspace.inspect".to_owned()).expect("name");
    let mutation = CapabilityName::new("workspace.mutate".to_owned()).expect("name");
    assert_eq!(
        registry.descriptor_for(&inspection).map(OperationDescriptor::operation_class),
        Some(OperationClass::Inspection)
    );
    assert!(registry.descriptor_for(&mutation).is_none());
    for role in ROLES {
        assert!(!registry.role_permits(role, &mutation), "unknown operation for {role:?}");
    }
}

fn registry_policy(
    ids: &FixtureIds,
    operation_name: &'static str,
    operations: peritus_policy::OperationRegistry,
) -> peritus_policy::PolicyDefinition {
    let permission = PermissionSpec { resource: ids.first_resource, name: operation_name };
    policy(PolicyInput {
        actors: vec![ids.actor],
        roles: vec![ActorRole::Reviewer],
        environments: vec![ids.environment],
        permissions: vec![permission],
        revision: ids.revision(),
        validity: window(1, 0, 100),
        uses: use_limit(Some(2)),
        grants: vec![grant(
            10,
            permission_selector(ids.revision(), vec![permission]),
            window(1, 0, 100),
            use_limit(Some(2)),
        )],
        immutable_denies: Vec::new(),
        operations,
        layers: Vec::new(),
    })
}

fn registry_request(
    ids: &FixtureIds,
    operation_name: &'static str,
) -> peritus_policy::AuthorizationRequest {
    peritus_policy::AuthorizationRequest::new(scope(ScopeInput {
        actor: ids.actor,
        role: ActorRole::Reviewer,
        environment: ids.environment,
        permissions: vec![PermissionSpec { resource: ids.first_resource, name: operation_name }],
        revision: ids.revision(),
        validity: window(1, 0, 100),
        uses: use_limit(Some(2)),
    }))
}

#[test]
fn unknown_and_self_downgraded_operations_fail_against_the_policy_bound_registry() {
    let ids = FixtureIds::new();
    let unknown_policy = registry_policy(&ids, "workspace.unknown", registry(Vec::new()));
    let unknown = unknown_policy
        .evaluate(
            registry_request(&ids, "workspace.unknown"),
            AuthorityTimeState::new(crate::support::instant(1, 0)),
            crate::support::instant(1, 10),
        )
        .expect("evaluation");
    assert_eq!(
        unknown.denial().map(peritus_policy::AuthorizationDenial::reason),
        Some(peritus_policy::AuthorizationDenialReason::UnknownOperation)
    );

    let mutation_policy = registry_policy(
        &ids,
        "workspace.mutate",
        registry(vec![descriptor("workspace.mutate", OperationClass::WorkspaceMutation)]),
    );
    let unrelated_downgrade =
        registry(vec![descriptor("workspace.mutate", OperationClass::Inspection)]);
    assert!(unrelated_downgrade.role_permits(
        ActorRole::Reviewer,
        &CapabilityName::new("workspace.mutate".to_owned()).expect("name")
    ));
    let denied = mutation_policy
        .evaluate(
            registry_request(&ids, "workspace.mutate"),
            AuthorityTimeState::new(crate::support::instant(1, 0)),
            crate::support::instant(1, 10),
        )
        .expect("evaluation");
    assert_eq!(
        denied.denial().map(peritus_policy::AuthorizationDenial::reason),
        Some(peritus_policy::AuthorizationDenialReason::RoleSeparation)
    );
}

#[test]
fn escalation_owns_the_exact_authenticated_risk_union_without_caller_downgrade() {
    let ids = FixtureIds::new();
    let permission = PermissionSpec { resource: ids.first_resource, name: "workspace.mutate" };
    let low_risk = OperationDescriptor::new(
        CapabilityName::new("workspace.mutate".to_owned()).expect("name"),
        OperationClass::WorkspaceMutation,
        RiskSet::new(vec![RiskClass::Read]).expect("canonical low-risk set"),
    )
    .expect_err("mandatory scoped-write risk cannot be omitted");
    assert_eq!(low_risk.kind(), PolicyErrorKind::InvalidOperationRisk);

    let descriptor = OperationDescriptor::new(
        CapabilityName::new("workspace.mutate".to_owned()).expect("name"),
        OperationClass::WorkspaceMutation,
        RiskSet::new(vec![RiskClass::ScopedWrite, RiskClass::Network])
            .expect("canonical authenticated risks"),
    )
    .expect("mandatory risk present");
    let definition = policy(PolicyInput {
        actors: vec![ids.actor],
        roles: vec![ActorRole::Writer],
        environments: vec![ids.environment],
        permissions: vec![permission],
        revision: ids.revision(),
        validity: window(1, 0, 100),
        uses: use_limit(Some(2)),
        grants: vec![grant(
            30,
            permission_selector(ids.revision(), vec![permission]),
            window(1, 0, 100),
            use_limit(Some(2)),
        )],
        immutable_denies: Vec::new(),
        operations: registry(vec![descriptor]),
        layers: vec![layer(
            PolicyTier::Project,
            vec![approval_rule(
                31,
                any_selector(ids.revision()),
                approval_requirement(
                    AuthorityTier::User,
                    vec![ActorRole::HumanAuthority],
                    vec![IndependenceRequirement::NotRequester],
                    window(1, 0, 100),
                ),
            )],
        )],
    });
    let decision = definition
        .evaluate(
            peritus_policy::AuthorizationRequest::new(scope(ScopeInput {
                actor: ids.actor,
                role: ActorRole::Writer,
                environment: ids.environment,
                permissions: vec![permission],
                revision: ids.revision(),
                validity: window(1, 0, 100),
                uses: use_limit(Some(2)),
            })),
            AuthorityTimeState::new(crate::support::instant(1, 0)),
            crate::support::instant(1, 10),
        )
        .expect("evaluation");
    let challenge = decision.escalation_challenge().expect("approval challenge");
    assert_eq!(challenge.risks().as_slice(), &[RiskClass::ScopedWrite, RiskClass::Network],);
}

#[test]
fn whole_request_risk_union_deduplicates_overlap_in_canonical_order() {
    let ids = FixtureIds::new();
    let mutation = PermissionSpec { resource: ids.second_resource, name: "workspace.mutate" };
    let network = PermissionSpec { resource: ids.first_resource, name: "network.call" };
    let mutation_descriptor = OperationDescriptor::new(
        CapabilityName::new("workspace.mutate".to_owned()).expect("name"),
        OperationClass::WorkspaceMutation,
        RiskSet::new(vec![RiskClass::ScopedWrite, RiskClass::Network]).expect("mutation risks"),
    )
    .expect("mutation descriptor");
    let network_descriptor = OperationDescriptor::new(
        CapabilityName::new("network.call".to_owned()).expect("name"),
        OperationClass::Network,
        RiskSet::new(vec![RiskClass::Network, RiskClass::SecretUse]).expect("network risks"),
    )
    .expect("network descriptor");
    // Fixture builders canonicalize these intentionally different assembly orders before the
    // checked public constructors see them; the union must be independent of caller assembly.
    let definition = policy(PolicyInput {
        actors: vec![ids.actor],
        roles: vec![ActorRole::Writer],
        environments: vec![ids.environment],
        permissions: vec![mutation, network],
        revision: ids.revision(),
        validity: window(1, 0, 100),
        uses: use_limit(Some(2)),
        grants: vec![grant(
            40,
            permission_selector(ids.revision(), vec![network, mutation]),
            window(1, 0, 100),
            use_limit(Some(2)),
        )],
        immutable_denies: Vec::new(),
        operations: registry(vec![mutation_descriptor, network_descriptor]),
        layers: vec![layer(
            PolicyTier::Project,
            vec![approval_rule(
                41,
                any_selector(ids.revision()),
                approval_requirement(
                    AuthorityTier::User,
                    vec![ActorRole::HumanAuthority],
                    Vec::new(),
                    window(1, 0, 100),
                ),
            )],
        )],
    });
    let decision = definition
        .evaluate(
            peritus_policy::AuthorizationRequest::new(scope(ScopeInput {
                actor: ids.actor,
                role: ActorRole::Writer,
                environment: ids.environment,
                permissions: vec![network, mutation],
                revision: ids.revision(),
                validity: window(1, 0, 100),
                uses: use_limit(Some(2)),
            })),
            AuthorityTimeState::new(crate::support::instant(1, 0)),
            crate::support::instant(1, 10),
        )
        .expect("evaluation");
    assert_eq!(
        decision.escalation_challenge().expect("challenge").risks().as_slice(),
        &[RiskClass::ScopedWrite, RiskClass::Network, RiskClass::SecretUse],
    );
}
