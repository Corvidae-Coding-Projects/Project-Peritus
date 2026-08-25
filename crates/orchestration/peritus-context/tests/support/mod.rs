#![allow(dead_code, reason = "different integration matrices use different helpers")]

use peritus_codec::sha256;
use peritus_context::{
    AuthorityClass, ContentKind, ContextGraph, ContextLimits, ContextNode, ContextNodeId,
    ContextNodeMetadata, Provenance, RequirementMode, RoleVisibility, TrustClass,
    bind_context_content,
};
use peritus_policy::ActorRole;
use peritus_role::ContextClass;

pub fn limits() -> ContextLimits {
    ContextLimits::new(32, 4_096, 16, 11).expect("test limits are valid")
}

pub fn id(byte: u8) -> ContextNodeId {
    ContextNodeId::new([byte; 16]).expect("nonzero fixture identifier")
}

pub fn roles(values: Vec<ActorRole>) -> RoleVisibility {
    RoleVisibility::new(values, limits()).expect("canonical fixture roles")
}

pub fn writer_roles() -> RoleVisibility {
    roles(vec![ActorRole::Writer])
}

#[allow(
    clippy::too_many_arguments,
    reason = "fixture exposes every ranking and security dimension"
)]
pub fn node(
    byte: u8,
    text: &str,
    provenance: Provenance,
    authority: AuthorityClass,
    trust: TrustClass,
    class: ContextClass,
    kind: ContentKind,
    tokens: u64,
    recency: u64,
    requirement: RequirementMode,
    priority: u16,
    visibility: RoleVisibility,
    dependencies: Vec<ContextNodeId>,
) -> ContextNode {
    let content = bind_context_content(text.as_bytes().to_vec(), sha256(text.as_bytes()), limits())
        .expect("fixture content is valid");
    let metadata = ContextNodeMetadata::new(
        id(byte),
        provenance,
        authority,
        trust,
        class,
        kind,
        tokens,
        recency,
        requirement,
        priority,
        visibility,
        dependencies,
        limits(),
    )
    .expect("fixture metadata is valid");
    ContextNode::new(metadata, content)
}

pub fn evidence_node(
    byte: u8,
    text: &str,
    tokens: u64,
    requirement: RequirementMode,
    dependencies: Vec<ContextNodeId>,
) -> ContextNode {
    node(
        byte,
        text,
        Provenance::Repository,
        AuthorityClass::NonAuthoritative,
        TrustClass::Constrained,
        ContextClass::RepositorySource,
        ContentKind::RepositorySource,
        tokens,
        u64::from(byte),
        requirement,
        0,
        writer_roles(),
        dependencies,
    )
}

pub fn graph(nodes: Vec<ContextNode>) -> ContextGraph {
    ContextGraph::new(nodes, limits()).expect("fixture graph is canonical")
}
