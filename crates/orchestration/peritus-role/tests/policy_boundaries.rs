//! Public first-error and complete role projection regressions.
use peritus_policy::{ActorRole, OperationClass};
use peritus_role::{
    CapabilityView, ContextClass, ContextClassSet, ReviewIndependenceView, RoleErrorKind,
    RoleProfile,
};
use peritus_spec::ReviewerIndependence;

#[test]
fn context_errors_retain_first_pair_and_complete_payload() {
    let duplicate = ContextClassSet::new(vec![
        ContextClass::RepositorySource,
        ContextClass::RepositorySource,
        ContextClass::ImmutablePolicy,
    ])
    .unwrap_err();
    assert_eq!(duplicate.kind(), RoleErrorKind::DuplicateValue);
    assert_eq!(duplicate.context_class_value(), Some(ContextClass::RepositorySource));
    assert_eq!(duplicate.operation_value(), None);
    let descending = ContextClassSet::new(vec![
        ContextClass::WorkspaceState,
        ContextClass::RepositorySource,
        ContextClass::RepositorySource,
    ])
    .unwrap_err();
    assert_eq!(descending.kind(), RoleErrorKind::NonCanonicalOrder);
    assert_eq!(descending.context_class_value(), Some(ContextClass::RepositorySource));
    assert_eq!(descending.operation_value(), None);
    let empty = ContextClassSet::new(Vec::new()).unwrap_err();
    assert_eq!(empty.kind(), RoleErrorKind::EmptyCollection);
    assert_eq!(empty.context_class_value(), None);
    assert_eq!(empty.operation_value(), None);
}

#[test]
fn operation_permission_and_pair_errors_preserve_input_order() {
    let denied = CapabilityView::new(
        ActorRole::Reviewer,
        vec![OperationClass::Inspection, OperationClass::Execution, OperationClass::Execution],
    )
    .unwrap_err();
    assert_eq!(denied.kind(), RoleErrorKind::OperationNotPermitted);
    assert_eq!(denied.operation_value(), Some(OperationClass::Execution));
    assert_eq!(denied.context_class_value(), None);
    let duplicate = CapabilityView::new(
        ActorRole::Reviewer,
        vec![OperationClass::Inspection, OperationClass::Inspection, OperationClass::Execution],
    )
    .unwrap_err();
    assert_eq!(duplicate.kind(), RoleErrorKind::DuplicateValue);
    assert_eq!(duplicate.operation_value(), Some(OperationClass::Inspection));
    assert_eq!(duplicate.context_class_value(), None);
    let descending = CapabilityView::new(
        ActorRole::Writer,
        vec![OperationClass::Execution, OperationClass::Inspection, OperationClass::Acceptance],
    )
    .unwrap_err();
    assert_eq!(descending.kind(), RoleErrorKind::NonCanonicalOrder);
    assert_eq!(descending.operation_value(), Some(OperationClass::Inspection));
    assert_eq!(descending.context_class_value(), None);
}

#[test]
fn cloned_review_context_keeps_private_and_mutating_material_excluded() {
    let original = RoleProfile::for_actor_role(ActorRole::Reviewer);
    let cloned = original.clone();
    drop(original);
    assert_eq!(cloned.actor_role(), ActorRole::Reviewer);
    assert!(cloned.context().requires_fresh_context());
    assert!(!cloned.context().allows_producer_ancestry());
    assert!(!cloned.context().visible().contains(ContextClass::HiddenReasoning));
    assert!(!cloned.context().visible().contains(ContextClass::MemoryEvidence));
    assert!(cloned.context().visible().contains(ContextClass::CandidateDiff));
    assert!(!cloned.capabilities().permits(OperationClass::WorkspaceMutation));
    assert!(!cloned.capabilities().permits(OperationClass::Execution));
}

#[test]
fn independence_projection_keeps_all_combinations_distinct() {
    for mask in 0u8..64 {
        let flags = [
            mask & 1 != 0,
            mask & 2 != 0,
            mask & 4 != 0,
            mask & 8 != 0,
            mask & 16 != 0,
            mask & 32 != 0,
        ];
        let requirements =
            ReviewerIndependence::new(flags[0], flags[1], flags[2], flags[3], flags[4], flags[5]);
        let view = ReviewIndependenceView::from_contract(requirements);
        assert_eq!(
            [
                view.distinct_reviewers(),
                view.independent_from_producer(),
                view.distinct_contexts(),
                view.distinct_model_families(),
                view.distinct_providers(),
                view.no_shared_ancestry()
            ],
            flags
        );
        assert!(view.fresh_context());
    }
}
