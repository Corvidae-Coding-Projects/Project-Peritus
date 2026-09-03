//! Digest-checked context selections for role-specific run-knowledge deltas.

use crate::{ContextError, ContextErrorKind, ContextGraph, ContextNodeId};
use peritus_role::RoleProfile;
use peritus_run_knowledge::{
    DeltaDelivery, DeltaPacket, KnowledgeAuthority, KnowledgeSection, KnowledgeSectionId,
    RunKnowledgeSnapshot,
};
use vstd::prelude::*;

verus! {

/// Canonical link from one knowledge section to its exact context node.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct KnowledgeContextLink {
    section_id: KnowledgeSectionId,
    node_id: ContextNodeId,
}

impl KnowledgeContextLink {
    /// Creates one explicit section-to-node binding.
    #[must_use]
    pub const fn new(section_id: KnowledgeSectionId, node_id: ContextNodeId) -> Self {
        Self { section_id, node_id }
    }

    /// Knowledge section identity.
    #[must_use]
    pub const fn section_id(self) -> KnowledgeSectionId { self.section_id }

    /// Exact context node carrying the section bytes.
    #[must_use]
    pub const fn node_id(self) -> ContextNodeId { self.node_id }
}

/// One role-visible context selection with complete run-knowledge provenance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReusableContextSelection {
    node_id: ContextNodeId,
    section: KnowledgeSection,
    delivery: DeltaDelivery,
}

impl ReusableContextSelection {
    /// Exact selected context node.
    #[must_use]
    pub const fn node_id(&self) -> ContextNodeId { self.node_id }

    /// Full workspace/source/conversation/candidate/role/sequence provenance.
    #[must_use]
    pub const fn section(&self) -> &KnowledgeSection { &self.section }

    /// Whether the selection carries changed facts, a current reference, or navigation.
    #[must_use]
    pub const fn delivery(&self) -> DeltaDelivery { self.delivery }
}

/// Binds a pure role delta to exact, visible context content in deterministic packet order.
///
/// # Errors
///
/// Rejects snapshot/packet identity disagreement, unordered or missing links, absent nodes, digest
/// mismatch, role-hidden nodes, and authority/delivery disagreement.
pub fn build_reusable_context_selections(
    graph: &ContextGraph,
    snapshot: &RunKnowledgeSnapshot,
    packet: &DeltaPacket,
    links: &[KnowledgeContextLink],
) -> Result<Vec<ReusableContextSelection>, ContextError> {
    if snapshot.role() != packet.role() || snapshot.candidate() != packet.candidate() {
        return Err(ContextError::plain(ContextErrorKind::KnowledgeRoleMismatch));
    }
    validate_links(links, snapshot.sections().len())?;
    let profile = RoleProfile::for_harness_role(packet.role());
    let entries = packet.entries();
    let mut selections = Vec::with_capacity(entries.len());
    let mut index = 0;
    while index < entries.len()
        invariant
            index <= entries.len(),
            selections.len() == index,
        decreases entries.len() - index,
    {
        let entry = entries[index];
        let Some(section) = snapshot.section(entry.section_id()) else {
            return Err(ContextError::plain(ContextErrorKind::KnowledgeSectionMissing));
        };
        let Some(link) = find_link(links, entry.section_id()) else {
            return Err(ContextError::plain(ContextErrorKind::KnowledgeContextLinkMissing));
        };
        let Some(node) = graph.node(link.node_id()) else {
            return Err(ContextError::node(ContextErrorKind::PlanNodeMissing, link.node_id()));
        };
        if node.digest() != section.section_digest() {
            return Err(ContextError::node(
                ContextErrorKind::KnowledgeContextDigestMismatch,
                link.node_id(),
            ));
        }
        if !node.visibility().contains(packet.role().actor_role())
            || !profile.context().visible().contains(node.context_class())
            || !delivery_matches_authority(entry.delivery(), section.authority())
        {
            return Err(ContextError::node(
                ContextErrorKind::KnowledgeRoleMismatch,
                link.node_id(),
            ));
        }
        selections.push(ReusableContextSelection {
            node_id: link.node_id(),
            section: section.clone(),
            delivery: entry.delivery(),
        });
        index += 1;
    }
    Ok(selections)
}

fn validate_links(links: &[KnowledgeContextLink], maximum: usize) -> Result<(), ContextError> {
    if links.len() > maximum {
        return Err(ContextError::with_numbers(
            ContextErrorKind::TooManyNodes,
            maximum as u64,
            links.len() as u64,
        ));
    }
    let mut index = 0;
    while index < links.len()
        invariant index <= links.len(),
        decreases links.len() - index,
    {
        if index > 0 {
            if links[index - 1].section_id() == links[index].section_id() {
                return Err(ContextError::plain(ContextErrorKind::DuplicateValue));
            }
            if links[index - 1].section_id() > links[index].section_id() {
                return Err(ContextError::plain(ContextErrorKind::NonCanonicalOrder));
            }
        }
        index += 1;
    }
    Ok(())
}

fn find_link(
    links: &[KnowledgeContextLink],
    id: KnowledgeSectionId,
) -> Option<KnowledgeContextLink> {
    let mut index = 0;
    while index < links.len()
        invariant index <= links.len(),
        decreases links.len() - index,
    {
        if links[index].section_id() == id {
            return Some(links[index]);
        }
        if links[index].section_id() > id {
            return None;
        }
        index += 1;
    }
    None
}

const fn delivery_matches_authority(
    delivery: DeltaDelivery,
    authority: KnowledgeAuthority,
) -> bool {
    matches!(
        (delivery, authority),
        (DeltaDelivery::ChangedFact | DeltaDelivery::CurrentReference, KnowledgeAuthority::Authoritative)
            | (DeltaDelivery::Navigation, KnowledgeAuthority::NavigationOnly)
    )
}

} // verus!
