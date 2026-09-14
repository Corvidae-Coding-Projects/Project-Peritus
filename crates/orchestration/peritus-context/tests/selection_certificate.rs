//! Adversarial mutations for the independent exact-selection replay certificate.

mod support;

use peritus_context::{
    AuthorityClass, ContentKind, ContextPlanId, Provenance, RequirementMode, SelectionPolicy,
    TokenBudget, TrustClass, select_context, selection_result_is_exact,
};
use peritus_policy::ActorRole;
use peritus_role::{ContextClass, HarnessRole, RoleProfile};
use peritus_types::Sha256Digest;
use support::{evidence_node, graph, id, node, roles, writer_roles};

const fn plan_id() -> ContextPlanId {
    ContextPlanId::new(Sha256Digest::new([71; 32]))
}

fn policy(tokens: u64, nodes: usize, bytes: usize) -> SelectionPolicy {
    SelectionPolicy::new(
        RoleProfile::for_harness_role(HarnessRole::Writer),
        TokenBudget::new(tokens + 2, 1, 1).expect("budget"),
        nodes,
        bytes,
    )
    .expect("policy")
}

#[test]
fn exact_success_and_exact_first_error_are_certified() {
    let success_graph = graph(vec![
        evidence_node(1, "dependency", 2, RequirementMode::DependencyRequired, Vec::new()),
        evidence_node(2, "root", 3, RequirementMode::Required, vec![id(1)]),
    ]);
    let success_policy = policy(5, 2, 100);
    let success = select_context(&success_graph, &success_policy, plan_id());
    assert!(selection_result_is_exact(&success_graph, &success_policy, plan_id(), &success,));

    let hidden = node(
        1,
        "hidden required",
        Provenance::Repository,
        AuthorityClass::NonAuthoritative,
        TrustClass::Constrained,
        ContextClass::RepositorySource,
        ContentKind::RepositorySource,
        1,
        1,
        RequirementMode::Required,
        0,
        roles(vec![ActorRole::Reviewer]),
        Vec::new(),
    );
    let error_graph = graph(vec![hidden]);
    let error_policy = policy(5, 2, 100);
    let error = select_context(&error_graph, &error_policy, plan_id());
    assert!(selection_result_is_exact(&error_graph, &error_policy, plan_id(), &error,));
}

#[test]
fn required_closure_reasons_and_render_order_cannot_be_substituted() {
    let required = graph(vec![
        evidence_node(1, "dependency", 2, RequirementMode::DependencyRequired, Vec::new()),
        evidence_node(2, "root", 3, RequirementMode::Required, vec![id(1)]),
    ]);
    let selection = policy(10, 4, 100);

    let missing_dependency =
        graph(vec![evidence_node(2, "root", 3, RequirementMode::Required, Vec::new())]);
    let incomplete = select_context(&missing_dependency, &selection, plan_id());
    assert!(!selection_result_is_exact(&required, &selection, plan_id(), &incomplete,));

    let optional = graph(vec![
        evidence_node(1, "dependency", 2, RequirementMode::DependencyRequired, Vec::new()),
        evidence_node(2, "root", 3, RequirementMode::Optional, vec![id(1)]),
    ]);
    let wrong_reasons = select_context(&optional, &selection, plan_id());
    assert!(!selection_result_is_exact(&required, &selection, plan_id(), &wrong_reasons,));

    let original_order = graph(vec![
        evidence_node(1, "repository", 1, RequirementMode::Optional, Vec::new()),
        node(
            2,
            "user",
            Provenance::User,
            AuthorityClass::UserInstruction,
            TrustClass::Trusted,
            ContextClass::ActiveUserRequest,
            ContentKind::ActiveUserInstruction,
            1,
            2,
            RequirementMode::Optional,
            0,
            writer_roles(),
            Vec::new(),
        ),
    ]);
    let reversed_order = graph(vec![
        node(
            1,
            "repository",
            Provenance::User,
            AuthorityClass::UserInstruction,
            TrustClass::Trusted,
            ContextClass::ActiveUserRequest,
            ContentKind::ActiveUserInstruction,
            1,
            1,
            RequirementMode::Optional,
            0,
            writer_roles(),
            Vec::new(),
        ),
        evidence_node(2, "user", 1, RequirementMode::Optional, Vec::new()),
    ]);
    let wrong_order = select_context(&reversed_order, &selection, plan_id());
    assert!(!selection_result_is_exact(&original_order, &selection, plan_id(), &wrong_order,));
}

#[test]
fn optional_omission_details_and_full_accounting_cannot_be_substituted() {
    let original_omission = graph(vec![
        evidence_node(1, "dependency", 4, RequirementMode::DependencyRequired, Vec::new()),
        evidence_node(2, "optional", 4, RequirementMode::Optional, vec![id(1)]),
    ]);
    let omission_policy = policy(6, 4, 100);
    let changed_tokens = graph(vec![
        evidence_node(1, "dependency", 3, RequirementMode::DependencyRequired, Vec::new()),
        evidence_node(2, "optional", 4, RequirementMode::Optional, vec![id(1)]),
    ]);
    let wrong_tokens = select_context(&changed_tokens, &omission_policy, plan_id());
    assert!(!selection_result_is_exact(
        &original_omission,
        &omission_policy,
        plan_id(),
        &wrong_tokens,
    ));

    let hidden_one = node(
        1,
        "hidden one",
        Provenance::Repository,
        AuthorityClass::NonAuthoritative,
        TrustClass::Constrained,
        ContextClass::RepositorySource,
        ContentKind::RepositorySource,
        1,
        1,
        RequirementMode::DependencyRequired,
        0,
        roles(vec![ActorRole::Reviewer]),
        Vec::new(),
    );
    let hidden_three = node(
        3,
        "hidden three",
        Provenance::Repository,
        AuthorityClass::NonAuthoritative,
        TrustClass::Constrained,
        ContextClass::RepositorySource,
        ContentKind::RepositorySource,
        1,
        3,
        RequirementMode::DependencyRequired,
        0,
        roles(vec![ActorRole::Reviewer]),
        Vec::new(),
    );
    let expected_blocker = graph(vec![
        hidden_one,
        evidence_node(2, "optional", 1, RequirementMode::Optional, vec![id(1)]),
    ]);
    let changed_blocker = graph(vec![
        evidence_node(2, "optional", 1, RequirementMode::Optional, vec![id(3)]),
        hidden_three,
    ]);
    let wrong_blocker = select_context(&changed_blocker, &omission_policy, plan_id());
    assert!(!selection_result_is_exact(
        &expected_blocker,
        &omission_policy,
        plan_id(),
        &wrong_blocker,
    ));

    let expected_accounting =
        graph(vec![evidence_node(1, "a", 3, RequirementMode::Required, Vec::new())]);
    let changed_accounting =
        graph(vec![evidence_node(1, "longer", 4, RequirementMode::Required, Vec::new())]);
    let accounting_policy = policy(10, 2, 100);
    let wrong_accounting = select_context(&changed_accounting, &accounting_policy, plan_id());
    assert!(!selection_result_is_exact(
        &expected_accounting,
        &accounting_policy,
        plan_id(),
        &wrong_accounting,
    ));
}

#[test]
fn optional_atomicity_ranking_and_first_error_precedence_cannot_be_substituted() {
    let closure = graph(vec![
        evidence_node(1, "dependency", 1, RequirementMode::DependencyRequired, Vec::new()),
        evidence_node(2, "optional", 1, RequirementMode::Optional, vec![id(1)]),
    ]);
    let admits = policy(2, 2, 100);
    let omits = policy(2, 1, 100);
    let partial_or_omitted = select_context(&closure, &omits, plan_id());
    assert!(!selection_result_is_exact(&closure, &admits, plan_id(), &partial_or_omitted,));

    let required = graph(vec![
        evidence_node(1, "dependency", 3, RequirementMode::DependencyRequired, Vec::new()),
        evidence_node(2, "root", 3, RequirementMode::Required, vec![id(1)]),
    ]);
    let token_first = policy(5, 1, 100);
    let node_first = policy(6, 1, 100);
    let wrong_error = select_context(&required, &node_first, plan_id());
    assert!(!selection_result_is_exact(&required, &token_first, plan_id(), &wrong_error,));

    let wrong_error_detail = select_context(&required, &policy(4, 1, 100), plan_id());
    assert!(!selection_result_is_exact(&required, &token_first, plan_id(), &wrong_error_detail,));
}
