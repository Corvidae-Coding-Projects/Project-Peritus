//! Compaction proposal and validation rejection matrix.

mod support;

use peritus_codec::sha256;
use peritus_context::{
    AuthorityClass, CompactionPolicy, CompactionPolicyId, CompactionProposal, ContentKind,
    ContextErrorKind, ContextPlanId, Provenance, RequirementMode, SelectionPolicy, SourceRange,
    TokenBudget, TrustClass, bind_context_content, replace_validated_compaction,
    validate_compaction,
};
use peritus_policy::ActorRole;
use peritus_role::{ContextClass, HarnessRole, RoleProfile};
use peritus_types::Sha256Digest;
use support::{evidence_node, graph as make_graph, id, limits, node, roles, writer_roles};

const fn compaction_policy(byte: u8, preserve: bool) -> CompactionPolicy {
    CompactionPolicy::new(CompactionPolicyId::new(Sha256Digest::new([byte; 32])), preserve)
}

fn selected_plan(graph: &peritus_context::ContextGraph) -> peritus_context::ContextPlan {
    let policy = SelectionPolicy::new(
        RoleProfile::for_harness_role(HarnessRole::Writer),
        TokenBudget::new(100, 10, 10).expect("budget"),
        32,
        4_096,
    )
    .expect("selection policy");
    peritus_context::select_context(graph, &policy, ContextPlanId::new(Sha256Digest::new([99; 32])))
        .expect("all fixture nodes fit")
}

fn proposal(
    output_id: u8,
    policy: CompactionPolicy,
    tokens: u64,
    ranges: Vec<SourceRange>,
) -> CompactionProposal {
    let content = bind_context_content(b"summary".to_vec(), sha256(b"summary"), limits())
        .expect("proposal content");
    CompactionProposal::new(id(output_id), policy.id(), content, tokens, 100, 7, ranges)
        .expect("proposal")
}

#[test]
fn ranges_and_proposals_reject_empty_overlap_duplicate_and_noncanonical_inputs() {
    assert_eq!(
        SourceRange::new(id(1), Sha256Digest::new([1; 32]), 2, 2).expect_err("empty range").kind(),
        ContextErrorKind::InvalidSourceRange
    );
    let digest = Sha256Digest::new([1; 32]);
    let first = SourceRange::new(id(1), digest, 0, 3).expect("range");
    let overlap = SourceRange::new(id(1), digest, 2, 4).expect("range");
    let content =
        bind_context_content(b"summary".to_vec(), sha256(b"summary"), limits()).expect("content");
    assert_eq!(
        CompactionProposal::new(
            id(9),
            compaction_policy(1, false).id(),
            content.clone(),
            1,
            1,
            0,
            vec![first, overlap],
        )
        .expect_err("overlap")
        .kind(),
        ContextErrorKind::OverlappingSourceRanges
    );
    assert_eq!(
        CompactionProposal::new(
            id(9),
            compaction_policy(1, false).id(),
            content,
            1,
            1,
            0,
            Vec::new(),
        )
        .expect_err("empty sources")
        .kind(),
        ContextErrorKind::EmptyCollection
    );
}

#[test]
fn successful_compaction_retains_provenance_ranges_dependencies_and_savings() {
    let graph = make_graph(vec![
        evidence_node(1, "source one", 8, RequirementMode::Optional, Vec::new()),
        evidence_node(2, "source two", 7, RequirementMode::Optional, Vec::new()),
    ]);
    let plan = selected_plan(&graph);
    let policy = compaction_policy(1, false);
    let ranges = vec![
        SourceRange::new(id(1), graph.node(id(1)).expect("source").digest(), 0, 6).expect("range"),
        SourceRange::new(id(2), graph.node(id(2)).expect("source").digest(), 0, 6).expect("range"),
    ];
    let validated = validate_compaction(
        &graph,
        &plan,
        &proposal(9, policy, 5, ranges.clone()),
        policy,
        limits(),
    )
    .expect("valid compaction");
    assert_eq!(validated.policy_id(), policy.id());
    assert_eq!(validated.source_ranges(), ranges);
    assert_eq!(validated.replaced_tokens(), 15);
    assert_eq!(validated.node().provenance(), Provenance::DerivedCompaction);
    assert_eq!(validated.node().authority(), AuthorityClass::NonAuthoritative);
    assert_eq!(validated.node().trust(), TrustClass::Untrusted);
    assert_eq!(validated.node().content_kind(), ContentKind::DerivedSummary);
    assert_eq!(validated.node().dependencies(), &[id(1), id(2)]);
}

#[test]
fn trust_is_preserved_only_for_all_trusted_inputs_and_an_explicit_policy() {
    let source = node(
        1,
        "trusted evidence",
        Provenance::System,
        AuthorityClass::NonAuthoritative,
        TrustClass::Trusted,
        ContextClass::RepositorySource,
        ContentKind::RepositorySource,
        8,
        1,
        RequirementMode::Optional,
        0,
        writer_roles(),
        Vec::new(),
    );
    let graph = make_graph(vec![source]);
    let plan = selected_plan(&graph);
    let range =
        SourceRange::new(id(1), graph.node(id(1)).expect("source").digest(), 0, 7).expect("range");
    for (preserve, expected) in [(false, TrustClass::Untrusted), (true, TrustClass::Trusted)] {
        let policy = compaction_policy(u8::from(preserve) + 1, preserve);
        let validated = validate_compaction(
            &graph,
            &plan,
            &proposal(9, policy, 2, vec![range]),
            policy,
            limits(),
        )
        .expect("trust policy");
        assert_eq!(validated.node().trust(), expected);
    }
}

#[test]
fn validation_rejects_policy_identity_existing_output_and_self_lineage() {
    let graph =
        make_graph(vec![evidence_node(1, "source", 8, RequirementMode::Optional, Vec::new())]);
    let plan = selected_plan(&graph);
    let digest = graph.node(id(1)).expect("source").digest();
    let range = SourceRange::new(id(1), digest, 0, 4).expect("range");
    let policy = compaction_policy(1, false);
    assert_eq!(
        validate_compaction(
            &graph,
            &plan,
            &proposal(9, policy, 1, vec![range]),
            compaction_policy(2, false),
            limits(),
        )
        .expect_err("wrong policy")
        .kind(),
        ContextErrorKind::CompactionPolicyMismatch
    );
    assert_eq!(
        validate_compaction(&graph, &plan, &proposal(1, policy, 1, vec![range]), policy, limits(),)
            .expect_err("self lineage")
            .kind(),
        ContextErrorKind::CompactionSourceCycle
    );
}

#[test]
fn validation_rejects_missing_digest_range_selection_and_savings_failures() {
    let graph =
        make_graph(vec![evidence_node(1, "source", 8, RequirementMode::Optional, Vec::new())]);
    let plan = selected_plan(&graph);
    let policy = compaction_policy(1, false);
    let cases = [
        (
            SourceRange::new(id(8), Sha256Digest::new([8; 32]), 0, 1).expect("range"),
            1,
            ContextErrorKind::MissingCompactionSource,
        ),
        (
            SourceRange::new(id(1), Sha256Digest::new([8; 32]), 0, 1).expect("range"),
            1,
            ContextErrorKind::DigestMismatch,
        ),
        (
            SourceRange::new(id(1), graph.node(id(1)).expect("source").digest(), 0, 99)
                .expect("range"),
            1,
            ContextErrorKind::InvalidSourceRange,
        ),
        (
            SourceRange::new(id(1), graph.node(id(1)).expect("source").digest(), 0, 1)
                .expect("range"),
            8,
            ContextErrorKind::CompactionNotSmaller,
        ),
    ];
    for (range, tokens, expected) in cases {
        let error = validate_compaction(
            &graph,
            &plan,
            &proposal(9, policy, tokens, vec![range]),
            policy,
            limits(),
        )
        .expect_err("rejection matrix");
        assert_eq!(error.kind(), expected);
    }

    let required = evidence_node(1, "required", 8, RequirementMode::Required, Vec::new());
    let optional = evidence_node(2, "optional", 8, RequirementMode::Optional, Vec::new());
    let graph = make_graph(vec![required, optional]);
    let selection = SelectionPolicy::new(
        RoleProfile::for_harness_role(HarnessRole::Writer),
        TokenBudget::new(18, 5, 5).expect("budget"),
        32,
        4_096,
    )
    .expect("policy");
    let plan = peritus_context::select_context(
        &graph,
        &selection,
        ContextPlanId::new(Sha256Digest::new([3; 32])),
    )
    .expect("optional omitted");
    let range =
        SourceRange::new(id(2), graph.node(id(2)).expect("source").digest(), 0, 1).expect("range");
    assert_eq!(
        validate_compaction(&graph, &plan, &proposal(9, policy, 1, vec![range]), policy, limits(),)
            .expect_err("not selected")
            .kind(),
        ContextErrorKind::CompactionSourceNotSelected
    );
}

#[test]
fn protected_and_mixed_context_sources_are_rejected() {
    let protected = node(
        1,
        "active request",
        Provenance::User,
        AuthorityClass::UserInstruction,
        TrustClass::Trusted,
        ContextClass::ActiveUserRequest,
        ContentKind::ActiveUserInstruction,
        8,
        1,
        RequirementMode::Optional,
        0,
        writer_roles(),
        Vec::new(),
    );
    let graph = make_graph(vec![protected]);
    let plan = selected_plan(&graph);
    let policy = compaction_policy(1, false);
    let range =
        SourceRange::new(id(1), graph.node(id(1)).expect("source").digest(), 0, 1).expect("range");
    assert_eq!(
        validate_compaction(&graph, &plan, &proposal(9, policy, 1, vec![range]), policy, limits(),)
            .expect_err("protected")
            .kind(),
        ContextErrorKind::ProtectedCompactionSource
    );

    let mixed = make_graph(vec![
        evidence_node(1, "repository", 8, RequirementMode::Optional, Vec::new()),
        node(
            2,
            "tool",
            Provenance::Tool,
            AuthorityClass::NonAuthoritative,
            TrustClass::Constrained,
            ContextClass::ToolObservation,
            ContentKind::ToolObservation,
            8,
            2,
            RequirementMode::Optional,
            0,
            writer_roles(),
            Vec::new(),
        ),
    ]);
    let plan = selected_plan(&mixed);
    let ranges = vec![
        SourceRange::new(id(1), mixed.node(id(1)).expect("source").digest(), 0, 1).expect("range"),
        SourceRange::new(id(2), mixed.node(id(2)).expect("source").digest(), 0, 1).expect("range"),
    ];
    assert_eq!(
        validate_compaction(&mixed, &plan, &proposal(9, policy, 1, ranges), policy, limits(),)
            .expect_err("mixed classes")
            .kind(),
        ContextErrorKind::IncompatibleCompactionClasses
    );
}

#[test]
fn hidden_source_is_rejected_even_with_a_plan_from_another_graph() {
    let visible =
        make_graph(vec![evidence_node(1, "same bytes", 8, RequirementMode::Optional, Vec::new())]);
    let plan = selected_plan(&visible);
    let hidden = make_graph(vec![node(
        1,
        "same bytes",
        Provenance::Repository,
        AuthorityClass::NonAuthoritative,
        TrustClass::Constrained,
        ContextClass::RepositorySource,
        ContentKind::RepositorySource,
        8,
        1,
        RequirementMode::Optional,
        0,
        roles(vec![ActorRole::Reviewer]),
        Vec::new(),
    )]);
    let policy = compaction_policy(1, false);
    let range =
        SourceRange::new(id(1), hidden.node(id(1)).expect("source").digest(), 0, 1).expect("range");
    assert_eq!(
        validate_compaction(
            &hidden,
            &plan,
            &proposal(9, policy, 1, vec![range]),
            policy,
            limits(),
        )
        .expect_err("source is hidden")
        .kind(),
        ContextErrorKind::HiddenCompactionSource
    );
}

#[test]
fn replacement_separates_lineage_and_rewrites_live_dependencies() {
    let graph = make_graph(vec![
        evidence_node(1, "external", 2, RequirementMode::Optional, Vec::new()),
        evidence_node(2, "source one", 8, RequirementMode::Optional, vec![id(1)]),
        evidence_node(3, "source two", 7, RequirementMode::Optional, vec![id(1), id(2)]),
        evidence_node(4, "dependent", 2, RequirementMode::Optional, vec![id(2), id(3)]),
    ]);
    let plan = selected_plan(&graph);
    let policy = compaction_policy(7, false);
    let ranges = vec![
        SourceRange::new(id(2), graph.node(id(2)).expect("source").digest(), 0, 6).expect("range"),
        SourceRange::new(id(3), graph.node(id(3)).expect("source").digest(), 0, 6).expect("range"),
    ];
    let validated = validate_compaction(
        &graph,
        &plan,
        &proposal(9, policy, 5, ranges.clone()),
        policy,
        limits(),
    )
    .expect("validation");
    let applied = replace_validated_compaction(&graph, validated).expect("replacement");

    assert_eq!(applied.source_ids(), &[id(2), id(3)]);
    assert_eq!(applied.source_ranges(), ranges);
    assert_eq!(applied.replaced_tokens(), 15);
    assert_eq!(applied.replacement_tokens(), 5);
    assert_eq!(
        applied.graph().nodes().iter().map(peritus_context::ContextNode::id).collect::<Vec<_>>(),
        vec![id(1), id(4), id(9)]
    );
    assert_eq!(applied.graph().node(id(9)).expect("replacement").dependencies(), &[id(1)]);
    assert_eq!(applied.graph().node(id(4)).expect("dependent").dependencies(), &[id(9)]);
}

#[test]
fn replacement_rejects_required_sources_and_graph_drift() {
    let required_graph =
        make_graph(vec![evidence_node(1, "required", 8, RequirementMode::Required, Vec::new())]);
    let plan = selected_plan(&required_graph);
    let policy = compaction_policy(8, false);
    let range = SourceRange::new(id(1), required_graph.node(id(1)).expect("source").digest(), 0, 4)
        .expect("range");
    let validated = validate_compaction(
        &required_graph,
        &plan,
        &proposal(9, policy, 1, vec![range]),
        policy,
        limits(),
    )
    .expect("validation remains backward compatible");
    assert_eq!(
        replace_validated_compaction(&required_graph, validated)
            .expect_err("required source")
            .kind(),
        ContextErrorKind::RequiredCompactionSource
    );

    let original =
        make_graph(vec![evidence_node(1, "source", 8, RequirementMode::Optional, Vec::new())]);
    let plan = selected_plan(&original);
    let range = SourceRange::new(id(1), original.node(id(1)).expect("source").digest(), 0, 4)
        .expect("range");
    let validated = validate_compaction(
        &original,
        &plan,
        &proposal(9, policy, 1, vec![range]),
        policy,
        limits(),
    )
    .expect("validation");
    let changed = make_graph(vec![evidence_node(
        1,
        "source changed",
        8,
        RequirementMode::Optional,
        Vec::new(),
    )]);
    assert_eq!(
        replace_validated_compaction(&changed, validated).expect_err("source drift").kind(),
        ContextErrorKind::CompactionSourceChanged
    );
}
