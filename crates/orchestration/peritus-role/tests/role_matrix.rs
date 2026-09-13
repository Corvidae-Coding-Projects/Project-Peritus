//! Exhaustive ordinary-Rust checks for C6 role and authority separation.

use peritus_policy::{ActorRole, OperationClass};
use peritus_role::{
    CapabilityView, ContextClass, ContextClassSet, HarnessRole, MemoryVisibility,
    ReasoningVisibility, RoleErrorKind, RoleProfile, capability_view_is_narrow,
    reviewer_context_is_fresh,
};

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

#[test]
fn every_b1_role_has_an_explicit_non_widening_profile() {
    for role in ROLES {
        let profile = RoleProfile::for_actor_role(role);
        assert_eq!(profile.actor_role(), role);
        assert!(capability_view_is_narrow(&profile));
        assert!(!profile.capabilities().operations().is_empty());
        for operation in profile.capabilities().operations() {
            assert!(role.permits_operation(*operation));
        }
    }
}

#[test]
fn harness_roles_map_exactly_to_b1() {
    let cases = [
        (HarnessRole::Writer, ActorRole::Writer),
        (HarnessRole::Reviewer, ActorRole::Reviewer),
        (HarnessRole::Fixer, ActorRole::Fixer),
        (HarnessRole::Evaluator, ActorRole::Evaluator),
        (HarnessRole::Evolver, ActorRole::EvolutionAgent),
    ];
    for (harness, actor) in cases {
        let profile = RoleProfile::for_harness_role(harness);
        assert_eq!(profile.actor_role(), actor);
        assert_eq!(profile.harness_role(), Some(harness));
    }
}

#[test]
fn writer_fixer_and_reviewer_separation_remains_exact() {
    for role in [HarnessRole::Writer, HarnessRole::Fixer] {
        let profile = RoleProfile::for_harness_role(role);
        assert!(!profile.capabilities().permits(OperationClass::Acceptance));
        assert!(!profile.capabilities().permits(OperationClass::Waiver));
        assert!(!profile.capabilities().permits(OperationClass::PolicyAmendment));
        assert!(!profile.capabilities().permits(OperationClass::HarnessPromotion));
    }

    let reviewer = RoleProfile::for_harness_role(HarnessRole::Reviewer);
    assert!(reviewer_context_is_fresh(&reviewer));
    assert_eq!(reviewer.context().memory_visibility(), MemoryVisibility::Excluded);
    assert_eq!(reviewer.context().reasoning_visibility(), ReasoningVisibility::Excluded);
    assert!(!reviewer.capabilities().permits(OperationClass::WorkspaceMutation));
    assert!(!reviewer.capabilities().permits(OperationClass::Execution));
}

#[test]
fn public_capability_constructor_rejects_widening_and_noncanonical_values() {
    let error = CapabilityView::new(ActorRole::Reviewer, vec![OperationClass::WorkspaceMutation])
        .expect_err("reviewer mutation must be denied");
    assert_eq!(error.kind(), RoleErrorKind::OperationNotPermitted);
    assert_eq!(error.operation_value(), Some(OperationClass::WorkspaceMutation));

    let error = CapabilityView::new(
        ActorRole::Writer,
        vec![OperationClass::Execution, OperationClass::Inspection],
    )
    .expect_err("unordered operations must fail");
    assert_eq!(error.kind(), RoleErrorKind::NonCanonicalOrder);
    assert_eq!(error.operation_value(), Some(OperationClass::Inspection));

    let error = CapabilityView::new(
        ActorRole::Writer,
        vec![OperationClass::Inspection, OperationClass::Inspection],
    )
    .expect_err("duplicate operations must fail");
    assert_eq!(error.kind(), RoleErrorKind::DuplicateValue);
    assert_eq!(error.operation_value(), Some(OperationClass::Inspection));
}

#[test]
fn every_required_and_contributable_class_is_visible() {
    for role in ROLES {
        let profile = RoleProfile::for_actor_role(role);
        for class in profile.context().required().values() {
            assert!(profile.context().visible().contains(*class));
        }
        for class in profile.context().contributable().values() {
            assert!(profile.context().visible().contains(*class));
        }
    }
}

#[test]
fn checked_context_sets_reject_empty_duplicates_and_unordered_values() {
    assert_eq!(
        ContextClassSet::new(Vec::new()).expect_err("empty class set must fail").kind(),
        RoleErrorKind::EmptyCollection
    );
    let duplicate =
        ContextClassSet::new(vec![ContextClass::ImmutablePolicy, ContextClass::ImmutablePolicy])
            .expect_err("duplicate class must fail");
    assert_eq!(duplicate.kind(), RoleErrorKind::DuplicateValue);
    assert_eq!(duplicate.context_class_value(), Some(ContextClass::ImmutablePolicy));

    let unordered = ContextClassSet::new(vec![
        ContextClass::WorkspaceState,
        ContextClass::AcceptanceSpecification,
    ])
    .expect_err("unordered classes must fail");
    assert_eq!(unordered.kind(), RoleErrorKind::NonCanonicalOrder);
    assert_eq!(unordered.context_class_value(), Some(ContextClass::AcceptanceSpecification));
}
