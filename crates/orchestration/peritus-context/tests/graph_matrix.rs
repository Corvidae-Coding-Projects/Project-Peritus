//! Constructor and canonical-DAG rejection matrix.

mod support;

use peritus_codec::sha256;
use peritus_context::{
    AuthorityClass, ContentKind, ContextErrorKind, ContextGraph, ContextLimits, ContextNodeId,
    ContextNodeMetadata, Provenance, RequirementMode, RoleVisibility, TrustClass,
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
    assert_eq!(
        ContextGraph::new(cycle, limits()).expect_err("cycle").kind(),
        ContextErrorKind::DependencyCycle
    );
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
