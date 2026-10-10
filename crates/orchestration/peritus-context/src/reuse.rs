//! Digest-checked context selections for role-specific run-knowledge deltas.

use crate::{ContextError, ContextErrorKind, ContextGraph, ContextNodeId};
use peritus_role::RoleProfile;
use peritus_run_knowledge::{
    DeltaDelivery, DeltaPacket, KnowledgeSection, KnowledgeSectionId, RunKnowledgeSnapshot,
};

mod build;

mod validation;

#[doc = " Canonical link from one knowledge section to its exact context node."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct KnowledgeContextLink {
    section_id: KnowledgeSectionId,
    node_id: ContextNodeId,
}
impl KnowledgeContextLink {
    #[doc = " Creates one explicit section-to-node binding."]
    #[must_use]
    pub const fn new(section_id: KnowledgeSectionId, node_id: ContextNodeId) -> Self {
        Self { section_id, node_id }
    }
    #[doc = " Knowledge section identity."]
    #[must_use]
    pub const fn section_id(self) -> KnowledgeSectionId {
        self.section_id
    }
    #[doc = " Exact context node carrying the section bytes."]
    #[must_use]
    pub const fn node_id(self) -> ContextNodeId {
        self.node_id
    }
}
#[doc = " One role-visible context selection with complete run-knowledge provenance."]
#[derive(Debug, Eq, PartialEq)]
pub struct ReusableContextSelection {
    node_id: ContextNodeId,
    section: KnowledgeSection,
    delivery: DeltaDelivery,
}
impl ReusableContextSelection {
    #[doc = " Exact selected context node."]
    #[must_use]
    pub const fn node_id(&self) -> ContextNodeId {
        self.node_id
    }
    #[doc = " Full workspace/source/conversation/candidate/role/sequence provenance."]
    #[must_use]
    pub const fn section(&self) -> &KnowledgeSection {
        &self.section
    }
    #[doc = " Whether the selection carries changed facts, a current reference, or navigation."]
    #[must_use]
    pub const fn delivery(&self) -> DeltaDelivery {
        self.delivery
    }
}
impl Clone for ReusableContextSelection {
    fn clone(&self) -> Self {
        Self { node_id: self.node_id, section: self.section.clone(), delivery: self.delivery }
    }
}
#[doc = " Binds a pure role delta to exact, visible context content in deterministic packet order."]
#[doc = ""]
#[doc = " # Errors"]
#[doc = ""]
#[doc = " Rejects snapshot/packet identity disagreement, unordered or missing links, absent nodes, digest"]
#[doc = " mismatch, role-hidden nodes, and authority/delivery disagreement."]
pub fn build_reusable_context_selections(
    graph: &ContextGraph,
    snapshot: &RunKnowledgeSnapshot,
    packet: &DeltaPacket,
    links: &[KnowledgeContextLink],
) -> Result<Vec<ReusableContextSelection>, ContextError> {
    let packet_role = packet.role();
    let roles_match = validation::roles_match(snapshot.role(), packet_role);
    let candidates_match = validation::candidates_match(snapshot.candidate(), packet.candidate());
    if !roles_match || !candidates_match {
        let error = ContextError::plain(ContextErrorKind::KnowledgeRoleMismatch);
        return Err(error);
    }
    let maximum_links = snapshot.sections().len();
    if links.len() > maximum_links {
        let error = ContextError::with_numbers(
            ContextErrorKind::TooManyNodes,
            maximum_links as u64,
            links.len() as u64,
        );
        return Err(error);
    }
    match validation::validate_link_order(links) {
        Ok(()) => {}
        Err(kind) => {
            let error = ContextError::plain(kind);
            return Err(error);
        }
    }
    let profile = RoleProfile::for_harness_role(packet_role);
    let entries = packet.entries();
    let mut selections = Vec::with_capacity(entries.len());
    let mut index = 0;
    while index < entries.len() {
        let entry = entries[index];
        match build::selection_for_entry(graph, snapshot, packet_role, &profile, links, entry) {
            Ok(selection) => selections.push(selection),
            Err(error) => {
                return Err(error);
            }
        }
        index += 1;
    }
    Ok(selections)
}
