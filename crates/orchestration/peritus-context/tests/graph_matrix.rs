//! Constructor and canonical-DAG rejection matrix.

mod support;

use peritus_codec::sha256;
use peritus_context::{
    AuthorityClass, ContentKind, ContextErrorKind, ContextGraph, ContextLimits, ContextNode,
    ContextNodeId, ContextNodeMetadata, Provenance, RequirementMode, RoleVisibility, TrustClass,
    bind_context_content,
};
use peritus_policy::ActorRole;
use peritus_role::ContextClass;
use support::{evidence_node, id, limits, roles, writer_roles};

#[test]
fn identifiers_content_and_limits_are_checked() {
    assert_eq!(
        ContextNodeId::new([0; 16]).expect_err("zero ID is reserved").kind(),
        ContextErrorKind::ZeroIdentifier
    );
    assert_eq!(
        ContextLimits::new(0, 1, 1, 1).expect_err("zero bound").kind(),
        ContextErrorKind::InvalidLimit
    );
    assert_eq!(
        bind_context_content(Vec::new(), sha256(b""), limits())
            .expect_err("content is nonempty")
            .kind(),
        ContextErrorKind::EmptyContent
    );
    assert_eq!(
        bind_context_content(b"x".to_vec(), sha256(b"y"), limits())
            .expect_err("digest must bind content")
            .kind(),
        ContextErrorKind::DigestMismatch
    );
    let tiny = ContextLimits::new(1, 1, 1, 1).expect("valid tiny limits");
    assert_eq!(
        bind_context_content(b"xx".to_vec(), sha256(b"xx"), tiny).expect_err("byte bound").kind(),
        ContextErrorKind::ContentTooLarge
    );
}

#[test]
fn visibility_requires_nonempty_canonical_unique_roles() {
    assert_eq!(
        RoleVisibility::new(Vec::new(), limits()).expect_err("empty").kind(),
        ContextErrorKind::EmptyCollection
    );
    assert_eq!(
        RoleVisibility::new(vec![ActorRole::Writer, ActorRole::Writer], limits())
            .expect_err("duplicate")
            .kind(),
        ContextErrorKind::DuplicateValue
    );
    assert_eq!(
        RoleVisibility::new(vec![ActorRole::Reviewer, ActorRole::Writer], limits())
            .expect_err("unordered")
            .kind(),
        ContextErrorKind::NonCanonicalOrder
    );
    assert!(roles(vec![ActorRole::Writer, ActorRole::Reviewer]).contains(ActorRole::Reviewer));
}

#[test]
fn node_metadata_rejects_zero_and_security_mismatches() {
    let make = |provenance, authority, trust, kind, tokens, recency| {
        ContextNodeMetadata::new(
            id(1),
            provenance,
            authority,
            trust,
            ContextClass::RepositorySource,
            kind,
            tokens,
            recency,
            RequirementMode::Optional,
            0,
            writer_roles(),
            Vec::new(),
            limits(),
        )
    };
    assert_eq!(
        make(
            Provenance::Repository,
            AuthorityClass::NonAuthoritative,
            TrustClass::Constrained,
            ContentKind::RepositorySource,
            0,
            1,
        )
        .expect_err("zero tokens")
        .kind(),
        ContextErrorKind::ZeroTokenEstimate
    );
    assert_eq!(
        make(
            Provenance::Repository,
            AuthorityClass::NonAuthoritative,
            TrustClass::Constrained,
            ContentKind::RepositorySource,
            1,
            0,
        )
        .expect_err("zero recency")
        .kind(),
        ContextErrorKind::ZeroRecency
    );
    assert_eq!(
        make(
            Provenance::External,
            AuthorityClass::ApplicationPolicy,
            TrustClass::Untrusted,
            ContentKind::ApplicationPolicy,
            1,
            1,
        )
        .expect_err("external authority promotion")
        .kind(),
        ContextErrorKind::IncompatibleAuthority
    );
    assert_eq!(
        make(
            Provenance::External,
            AuthorityClass::NonAuthoritative,
            TrustClass::Trusted,
            ContentKind::RepositorySource,
            1,
            1,
        )
        .expect_err("external trust promotion")
        .kind(),
        ContextErrorKind::IncompatibleTrust
    );
    assert_eq!(
        make(
            Provenance::System,
            AuthorityClass::NonAuthoritative,
            TrustClass::Trusted,
            ContentKind::SystemPolicy,
            1,
            1,
        )
        .expect_err("protected kind requires exact authority")
        .kind(),
        ContextErrorKind::IncompatibleContentKind
    );
}

#[test]
fn dependency_metadata_rejects_self_duplicate_and_noncanonical_edges() {
    let build = |dependencies| {
        ContextNodeMetadata::new(
            id(2),
            Provenance::Repository,
            AuthorityClass::NonAuthoritative,
            TrustClass::Constrained,
            ContextClass::RepositorySource,
            ContentKind::RepositorySource,
            1,
            1,
            RequirementMode::Optional,
            0,
            writer_roles(),
            dependencies,
            limits(),
        )
    };
    assert_eq!(build(vec![id(2)]).expect_err("self edge").kind(), ContextErrorKind::SelfDependency);
    assert_eq!(
        build(vec![id(1), id(1)]).expect_err("duplicate edge").kind(),
        ContextErrorKind::DuplicateValue
    );
    assert_eq!(
        build(vec![id(3), id(1)]).expect_err("unordered edge").kind(),
        ContextErrorKind::NonCanonicalOrder
    );
}

#[test]
fn graph_rejects_empty_duplicate_unordered_and_missing_nodes() {
    assert_eq!(
        ContextGraph::new(Vec::new(), limits()).expect_err("empty graph").kind(),
        ContextErrorKind::EmptyCollection
    );
    let first = evidence_node(1, "one", 1, RequirementMode::Optional, Vec::new());
    assert_eq!(
        ContextGraph::new(vec![first.clone(), first], limits()).expect_err("duplicate IDs").kind(),
        ContextErrorKind::DuplicateValue
    );
    assert_eq!(
        ContextGraph::new(
            vec![
                evidence_node(2, "two", 1, RequirementMode::Optional, Vec::new()),
                evidence_node(1, "one", 1, RequirementMode::Optional, Vec::new()),
            ],
            limits(),
        )
        .expect_err("canonical graph order")
        .kind(),
        ContextErrorKind::NonCanonicalOrder
    );
    let missing = evidence_node(1, "one", 1, RequirementMode::Optional, vec![id(9)]);
    let error = ContextGraph::new(vec![missing], limits()).expect_err("missing edge");
    assert_eq!(error.kind(), ContextErrorKind::MissingDependency);
    assert_eq!(error.related_id(), Some(id(9)));
}

#[test]
fn graph_rejects_cycles_and_accepts_a_canonical_dag() {
    let cycle = vec![
        evidence_node(1, "one", 1, RequirementMode::Optional, vec![id(2)]),
        evidence_node(2, "two", 1, RequirementMode::Optional, vec![id(1)]),
    ];
    let cycle_error = ContextGraph::new(cycle, limits()).expect_err("cycle");
    assert_eq!(cycle_error.kind(), ContextErrorKind::DependencyCycle);
    assert_eq!(cycle_error.node_id(), Some(id(1)));
    let dag = ContextGraph::new(
        vec![
            evidence_node(1, "one", 1, RequirementMode::DependencyRequired, Vec::new()),
            evidence_node(2, "two", 1, RequirementMode::Required, vec![id(1)]),
        ],
        limits(),
    )
    .expect("valid DAG");
    assert_eq!(dag.nodes().len(), 2);
    assert_eq!(dag.node(id(1)).expect("indexed node").content().bytes(), b"one");
}

fn legacy_cycle_member(nodes: &[ContextNode]) -> Option<ContextNodeId> {
    let mut indegree = vec![0_usize; nodes.len()];
    for node in nodes {
        for dependency in node.dependencies() {
            let Some(target) = nodes.iter().position(|candidate| candidate.id() == *dependency)
            else {
                return Some(node.id());
            };
            let Some(next) = indegree[target].checked_add(1) else {
                return Some(nodes[target].id());
            };
            indegree[target] = next;
        }
    }

    let mut removed = vec![false; nodes.len()];
    let mut removed_count = 0_usize;
    while removed_count < nodes.len() {
        let Some(index) = (0..nodes.len()).find(|index| !removed[*index] && indegree[*index] == 0)
        else {
            break;
        };
        removed[index] = true;
        removed_count += 1;
        for dependency in nodes[index].dependencies() {
            let Some(target) = nodes.iter().position(|candidate| candidate.id() == *dependency)
            else {
                return Some(nodes[index].id());
            };
            let Some(next) = indegree[target].checked_sub(1) else {
                return Some(nodes[target].id());
            };
            indegree[target] = next;
        }
    }
    removed.iter().position(|removed| !removed).map(|index| nodes[index].id())
}

fn assert_cycle_diagnostic_matches_legacy(nodes: Vec<ContextNode>, expected: ContextNodeId) {
    assert_eq!(legacy_cycle_member(nodes.as_slice()), Some(expected));
    let error = ContextGraph::new(nodes, limits()).expect_err("cycle must be rejected");
    assert_eq!(error.kind(), ContextErrorKind::DependencyCycle);
    assert_eq!(error.node_id(), Some(expected));
}

#[test]
fn cycle_diagnostics_preserve_legacy_residual_order_for_complex_shapes() {
    assert_cycle_diagnostic_matches_legacy(
        vec![
            evidence_node(1, "dependency tail", 1, RequirementMode::Optional, Vec::new()),
            evidence_node(2, "cycle a", 1, RequirementMode::Optional, vec![id(1), id(3)]),
            evidence_node(3, "cycle b", 1, RequirementMode::Optional, vec![id(2)]),
            evidence_node(4, "dependent tail", 1, RequirementMode::Optional, vec![id(3)]),
            evidence_node(5, "independent", 1, RequirementMode::Optional, Vec::new()),
        ],
        id(1),
    );

    assert_cycle_diagnostic_matches_legacy(
        vec![
            evidence_node(1, "cycle a1", 1, RequirementMode::Optional, vec![id(2)]),
            evidence_node(2, "cycle a2", 1, RequirementMode::Optional, vec![id(1)]),
            evidence_node(3, "cycle b1", 1, RequirementMode::Optional, vec![id(4)]),
            evidence_node(4, "cycle b2", 1, RequirementMode::Optional, vec![id(3)]),
        ],
        id(1),
    );

    assert_cycle_diagnostic_matches_legacy(
        vec![
            evidence_node(1, "first leaf", 1, RequirementMode::Optional, Vec::new()),
            evidence_node(2, "second leaf", 1, RequirementMode::Optional, vec![id(3)]),
            evidence_node(3, "cycle a", 1, RequirementMode::Optional, vec![id(4)]),
            evidence_node(4, "cycle b", 1, RequirementMode::Optional, vec![id(3)]),
        ],
        id(3),
    );
}

#[test]
fn dependent_count_traversal_preserves_long_chain_and_cycle_result() {
    const NODE_COUNT: u8 = 128;
    let graph_limits =
        ContextLimits::new(usize::from(NODE_COUNT), 4_096, 16, 11).expect("long-chain limits");

    let chain = (1..=NODE_COUNT)
        .map(|byte| {
            let dependencies = if byte == 1 { Vec::new() } else { vec![id(byte - 1)] };
            evidence_node(byte, "long chain", 1, RequirementMode::Optional, dependencies)
        })
        .collect();
    let graph = ContextGraph::new(chain, graph_limits).expect("long chain is acyclic");
    assert_eq!(graph.nodes().len(), usize::from(NODE_COUNT));

    let cycle = (1..=NODE_COUNT)
        .map(|byte| {
            let dependency = if byte == 1 { id(NODE_COUNT) } else { id(byte - 1) };
            evidence_node(byte, "long cycle", 1, RequirementMode::Optional, vec![dependency])
        })
        .collect();
    let error = ContextGraph::new(cycle, graph_limits).expect_err("long cycle is rejected");
    assert_eq!(error.kind(), ContextErrorKind::DependencyCycle);
    assert_eq!(error.node_id(), Some(id(1)));
}
