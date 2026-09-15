//! Required-closure, ranking, atomic omission, and token-accounting matrix.

mod support;

use peritus_context::{
    ContextErrorKind, ContextPlanId, OmissionReason, RequirementMode, SelectionPolicy,
    SelectionReason, TokenBudget, plan_dependencies_complete, plan_is_exact, plan_is_visible,
    select_context, token_accounting_is_bounded,
};
use peritus_policy::ActorRole;
use peritus_role::{HarnessRole, RoleProfile};
use peritus_types::Sha256Digest;
use support::{evidence_node, graph, id, node, roles, writer_roles};

const fn plan_id() -> ContextPlanId {
    ContextPlanId::new(Sha256Digest::new([42; 32]))
}

fn policy(tokens: u64, nodes: usize, bytes: usize) -> SelectionPolicy {
    SelectionPolicy::new(
        RoleProfile::for_harness_role(HarnessRole::Writer),
        TokenBudget::new(tokens + 10, 5, 5).expect("budget"),
        nodes,
        bytes,
    )
    .expect("selection policy")
}

#[test]
fn token_budget_checks_boundaries_and_reports_each_component() {
    assert_eq!(
        TokenBudget::new(10, 8, 3).expect_err("reservations exceed window").kind(),
        ContextErrorKind::InvalidTokenBudget
    );
    assert_eq!(
        TokenBudget::new(u64::MAX, u64::MAX, 1).expect_err("reservation addition overflow").kind(),
        ContextErrorKind::ArithmeticOverflow
    );
    let budget = TokenBudget::new(100, 20, 10).expect("valid budget");
    assert_eq!(budget.context_window(), 100);
    assert_eq!(budget.reserved_output(), 20);
    assert_eq!(budget.reserved_protocol_overhead(), 10);
    assert_eq!(budget.usable_input(), 70);
}

#[test]
fn required_root_selects_complete_dependency_closure() {
    let graph = graph(vec![
        evidence_node(1, "dependency", 3, RequirementMode::DependencyRequired, Vec::new()),
        evidence_node(2, "root", 5, RequirementMode::Required, vec![id(1)]),
    ]);
    let plan = select_context(&graph, &policy(8, 2, 100), plan_id()).expect("exact fit");
    assert!(plan.contains(id(1)));
    assert!(plan.contains(id(2)));
    assert!(plan_dependencies_complete(&graph, &plan));
    assert!(plan_is_visible(&graph, &plan));
    assert!(plan_is_exact(&graph, &plan));
    assert!(token_accounting_is_bounded(plan.accounting()));
    assert_eq!(plan.accounting().used_input(), 8);
    assert_eq!(plan.accounting().remaining_input(), 0);
    assert_eq!(
        plan.selected().iter().find(|entry| entry.node_id() == id(1)).unwrap().reason(),
        SelectionReason::RequiredDependency
    );
    assert_eq!(
        plan.selected().iter().find(|entry| entry.node_id() == id(2)).unwrap().reason(),
        SelectionReason::RequiredRoot
    );
}

#[test]
fn hidden_required_root_and_dependency_fail_transactionally() {
    let hidden_root = node(
        1,
        "hidden",
        peritus_context::Provenance::Repository,
        peritus_context::AuthorityClass::NonAuthoritative,
        peritus_context::TrustClass::Constrained,
        peritus_role::ContextClass::RepositorySource,
        peritus_context::ContentKind::RepositorySource,
        1,
        1,
        RequirementMode::Required,
        0,
        roles(vec![ActorRole::Reviewer]),
        Vec::new(),
    );
    assert_eq!(
        select_context(&graph(vec![hidden_root]), &policy(10, 10, 100), plan_id())
            .expect_err("hidden root")
            .kind(),
        ContextErrorKind::HiddenRequiredNode
    );

    let hidden_dependency = node(
        1,
        "hidden dependency",
        peritus_context::Provenance::Repository,
        peritus_context::AuthorityClass::NonAuthoritative,
        peritus_context::TrustClass::Constrained,
        peritus_role::ContextClass::RepositorySource,
        peritus_context::ContentKind::RepositorySource,
        1,
        1,
        RequirementMode::DependencyRequired,
        0,
        roles(vec![ActorRole::Reviewer]),
        Vec::new(),
    );
    let root = evidence_node(2, "root", 1, RequirementMode::Required, vec![id(1)]);
    let error =
        select_context(&graph(vec![hidden_dependency, root]), &policy(10, 10, 100), plan_id())
            .expect_err("hidden dependency");
    assert_eq!(error.kind(), ContextErrorKind::HiddenRequiredDependency);
    assert_eq!(error.node_id(), Some(id(2)));
    assert_eq!(error.related_id(), Some(id(1)));
}

#[test]
fn required_budget_node_and_byte_failures_name_the_root() {
    let single_graph =
        graph(vec![evidence_node(1, "required", 5, RequirementMode::Required, Vec::new())]);
    for (selection, expected) in [
        (policy(4, 10, 100), ContextErrorKind::RequiredTokenBudgetExceeded),
        (policy(10, 10, 1), ContextErrorKind::RequiredByteLimitExceeded),
    ] {
        let error = select_context(&single_graph, &selection, plan_id())
            .expect_err("required bound must fail");
        assert_eq!(error.kind(), expected);
        assert_eq!(error.node_id(), Some(id(1)));
    }
    let graph = graph(vec![
        evidence_node(1, "a", 1, RequirementMode::Required, Vec::new()),
        evidence_node(2, "b", 1, RequirementMode::Required, Vec::new()),
    ]);
    let error = select_context(&graph, &policy(10, 1, 100), plan_id())
        .expect_err("second required root exceeds node limit");
    assert_eq!(error.kind(), ContextErrorKind::RequiredNodeLimitExceeded);
    assert_eq!(error.node_id(), Some(id(2)));
}

#[test]
fn optional_closure_is_omitted_atomically_and_explained() {
    let graph = graph(vec![
        evidence_node(1, "dependency", 4, RequirementMode::DependencyRequired, Vec::new()),
        node(
            2,
            "optional user root",
            peritus_context::Provenance::User,
            peritus_context::AuthorityClass::UserInstruction,
            peritus_context::TrustClass::Trusted,
            peritus_role::ContextClass::ActiveUserRequest,
            peritus_context::ContentKind::ActiveUserInstruction,
            4,
            2,
            RequirementMode::Optional,
            0,
            writer_roles(),
            vec![id(1)],
        ),
        evidence_node(3, "required", 3, RequirementMode::Required, Vec::new()),
    ]);
    let plan = select_context(&graph, &policy(3, 10, 100), plan_id()).expect("optional omission");
    assert!(plan.contains(id(3)));
    assert!(!plan.contains(id(1)));
    assert!(!plan.contains(id(2)));
    assert_eq!(plan.omitted().len(), 1);
    assert_eq!(plan.omitted()[0].node_id(), id(2));
    assert_eq!(plan.omitted()[0].reason(), OmissionReason::TokenBudget);
    assert_eq!(plan.omitted()[0].required_tokens(), 8);
}

#[test]
fn dependency_required_nodes_are_never_ranked_as_standalone_optional_roots() {
    let graph = graph(vec![evidence_node(
        1,
        "dependency-only",
        1,
        RequirementMode::DependencyRequired,
        Vec::new(),
    )]);
    let plan = select_context(&graph, &policy(10, 10, 100), plan_id()).expect("empty plan");

    assert!(plan.selected().is_empty());
    assert!(plan.omitted().is_empty());
    assert_eq!(plan.accounting().used_input(), 0);
    assert_eq!(plan.selected_bytes(), 0);
}

#[test]
fn optional_omission_uses_the_first_capacity_failure_and_names_only_the_root() {
    let graph = graph(vec![
        evidence_node(1, "dependency", 4, RequirementMode::DependencyRequired, Vec::new()),
        evidence_node(2, "optional", 4, RequirementMode::Optional, vec![id(1)]),
    ]);
    let cases = [
        (policy(7, 1, 1), OmissionReason::TokenBudget),
        (policy(8, 1, 1), OmissionReason::NodeLimit),
        (policy(8, 2, 1), OmissionReason::ByteLimit),
    ];

    for (selection, expected) in cases {
        let plan = select_context(&graph, &selection, plan_id()).expect("optional omission");
        assert!(plan.selected().is_empty());
        assert_eq!(plan.omitted().len(), 1);
        assert_eq!(plan.omitted()[0].node_id(), id(2));
        assert_eq!(plan.omitted()[0].reason(), expected);
        assert_eq!(plan.omitted()[0].blocking_dependency(), None);
        assert_eq!(plan.omitted()[0].required_tokens(), 8);
        assert_eq!(plan.accounting().used_input(), 0);
        assert_eq!(plan.selected_bytes(), 0);
    }
}

#[test]
fn an_optional_root_with_a_hidden_dependency_is_omitted_before_capacity_checks() {
    let hidden_dependency = node(
        1,
        "hidden dependency",
        peritus_context::Provenance::Repository,
        peritus_context::AuthorityClass::NonAuthoritative,
        peritus_context::TrustClass::Constrained,
        peritus_role::ContextClass::RepositorySource,
        peritus_context::ContentKind::RepositorySource,
        100,
        1,
        RequirementMode::DependencyRequired,
        0,
        roles(vec![ActorRole::Reviewer]),
        Vec::new(),
    );
    let root = evidence_node(2, "optional", 100, RequirementMode::Optional, vec![id(1)]);
    let plan = select_context(&graph(vec![hidden_dependency, root]), &policy(1, 1, 1), plan_id())
        .expect("hidden optional dependency is an omission");

    assert!(plan.selected().is_empty());
    assert_eq!(plan.omitted().len(), 1);
    assert_eq!(plan.omitted()[0].node_id(), id(2));
    assert_eq!(plan.omitted()[0].reason(), OmissionReason::HiddenDependency);
    assert_eq!(plan.omitted()[0].blocking_dependency(), Some(id(1)));
    assert_eq!(plan.omitted()[0].required_tokens(), 0);
}

#[test]
fn ranking_uses_authority_requirement_priority_recency_then_id() {
    let high_priority = node(
        1,
        "high priority repository",
        peritus_context::Provenance::Repository,
        peritus_context::AuthorityClass::NonAuthoritative,
        peritus_context::TrustClass::Constrained,
        peritus_role::ContextClass::RepositorySource,
        peritus_context::ContentKind::RepositorySource,
        2,
        100,
        RequirementMode::Optional,
        u16::MAX,
        writer_roles(),
        Vec::new(),
    );
    let user = node(
        2,
        "user",
        peritus_context::Provenance::User,
        peritus_context::AuthorityClass::UserInstruction,
        peritus_context::TrustClass::Trusted,
        peritus_role::ContextClass::ActiveUserRequest,
        peritus_context::ContentKind::ActiveUserInstruction,
        2,
        1,
        RequirementMode::Optional,
        0,
        writer_roles(),
        Vec::new(),
    );
    let graph = graph(vec![high_priority, user]);
    let plan = select_context(&graph, &policy(2, 10, 100), plan_id()).expect("one optional fits");
    assert!(plan.contains(id(2)), "authority outranks explicit priority");
    assert!(!plan.contains(id(1)));
}

#[test]
fn identical_inputs_produce_byte_for_byte_equal_plans() {
    let graph = graph(vec![
        evidence_node(1, "a", 1, RequirementMode::Optional, Vec::new()),
        evidence_node(2, "b", 1, RequirementMode::Optional, Vec::new()),
        evidence_node(3, "c", 1, RequirementMode::Required, Vec::new()),
    ]);
    let policy = policy(2, 3, 100);
    let first = select_context(&graph, &policy, plan_id()).expect("plan");
    let second = select_context(&graph, &policy, plan_id()).expect("same plan");
    assert_eq!(first, second);
}
