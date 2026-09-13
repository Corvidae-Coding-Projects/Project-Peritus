//! Digest-checked context selections for role-specific run-knowledge deltas.

use crate::{ContextError, ContextErrorKind, ContextGraph, ContextNodeId};
use peritus_role::RoleProfile;
use peritus_run_knowledge::{
    DeltaDelivery, DeltaPacket, KnowledgeSection, KnowledgeSectionId, RunKnowledgeSnapshot,
};
use vstd::prelude::*;

mod build;
mod model;
mod validation;

verus! {

/// Canonical link from one knowledge section to its exact context node.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct KnowledgeContextLink {
    section_id: KnowledgeSectionId,
    node_id: ContextNodeId,
}

impl KnowledgeContextLink {
    /// Logical view of the exact section identity.
    pub closed spec fn spec_section_id(&self) -> KnowledgeSectionId { self.section_id }

    /// Logical view of the exact context-node identity.
    pub closed spec fn spec_node_id(&self) -> ContextNodeId { self.node_id }

    /// Creates one explicit section-to-node binding.
    #[must_use]
    pub const fn new(
        section_id: KnowledgeSectionId,
        node_id: ContextNodeId,
    ) -> (result: Self)
        ensures
            result.spec_section_id() == section_id,
            result.spec_node_id() == node_id,
    {
        Self { section_id, node_id }
    }

    /// Knowledge section identity.
    #[must_use]
    pub const fn section_id(self) -> (id: KnowledgeSectionId)
        ensures id == self.spec_section_id(),
    { self.section_id }

    /// Exact context node carrying the section bytes.
    #[must_use]
    pub const fn node_id(self) -> (id: ContextNodeId)
        ensures id == self.spec_node_id(),
    { self.node_id }
}

/// One role-visible context selection with complete run-knowledge provenance.
#[derive(Debug, Eq, PartialEq)]
pub struct ReusableContextSelection {
    node_id: ContextNodeId,
    section: KnowledgeSection,
    delivery: DeltaDelivery,
}

impl ReusableContextSelection {
    /// Logical view of the selected node identity.
    pub closed spec fn spec_node_id(&self) -> ContextNodeId { self.node_id }

    /// Logical view of complete retained knowledge provenance.
    pub closed spec fn spec_section(&self) -> KnowledgeSection { self.section }

    /// Logical view of the exact packet delivery classification.
    pub closed spec fn spec_delivery(&self) -> DeltaDelivery { self.delivery }

    /// Complete semantic fields preserved by cloning.
    pub open spec fn clone_equivalent(left: &Self, right: &Self) -> bool {
        left.spec_node_id() == right.spec_node_id()
            && KnowledgeSection::clone_equivalent(&left.spec_section(), &right.spec_section())
            && left.spec_delivery() == right.spec_delivery()
    }

    /// Exact selected context node.
    #[must_use]
    pub const fn node_id(&self) -> (id: ContextNodeId)
        ensures id == self.spec_node_id(),
    { self.node_id }

    /// Full workspace/source/conversation/candidate/role/sequence provenance.
    #[must_use]
    pub const fn section(&self) -> (section: &KnowledgeSection)
        ensures
            section.spec_id() == self.spec_section().spec_id(),
            section.spec_kind() == self.spec_section().spec_kind(),
            section.spec_section_digest() == self.spec_section().spec_section_digest(),
            section.spec_binding() == self.spec_section().spec_binding(),
            section.spec_dependencies() == self.spec_section().spec_dependencies(),
    { &self.section }

    /// Whether the selection carries changed facts, a current reference, or navigation.
    #[must_use]
    pub const fn delivery(&self) -> (delivery: DeltaDelivery)
        ensures delivery == self.spec_delivery(),
    { self.delivery }
}

impl Clone for ReusableContextSelection {
    fn clone(&self) -> (result: Self)
        ensures Self::clone_equivalent(self, &result),
    {
        Self {
            node_id: self.node_id,
            section: self.section.clone(),
            delivery: self.delivery,
        }
    }
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
) -> (result: Result<Vec<ReusableContextSelection>, ContextError>)
    ensures
        result.is_ok() == model::inputs_valid(graph, snapshot, packet, links@),
        match result {
            Ok(selections) => model::selections_refine(
                graph, snapshot, links@, packet.spec_entries(), selections@),
            Err(error) => model::construction_error(
                graph, snapshot, packet, links@, error),
        },
{
    let packet_role = packet.role();
    assert(packet_role == packet.spec_role());
    let roles_match = validation::roles_match(snapshot.role(), packet_role);
    let candidates_match =
        validation::candidates_match(snapshot.candidate(), packet.candidate());
    if !roles_match || !candidates_match {
        let error = ContextError::plain(ContextErrorKind::KnowledgeRoleMismatch);
        assert(!model::packet_binding_matches(snapshot, packet));
        assert(!model::inputs_valid(graph, snapshot, packet, links@));
        assert(model::construction_error(graph, snapshot, packet, links@, error));
        return Err(error);
    }
    assert(model::packet_binding_matches(snapshot, packet));
    let maximum_links = snapshot.sections().len();
    if links.len() > maximum_links {
        let error = ContextError::with_numbers(
            ContextErrorKind::TooManyNodes,
            maximum_links as u64,
            links.len() as u64,
        );
        assert(!model::links_valid(links@, snapshot.spec_sections().len()));
        assert(!model::inputs_valid(graph, snapshot, packet, links@));
        assert(model::construction_error(graph, snapshot, packet, links@, error));
        return Err(error);
    }
    match validation::validate_link_order(links) {
        Ok(()) => {}
        Err(kind) => {
            let error = ContextError::plain(kind);
            assert(!model::inputs_valid(graph, snapshot, packet, links@));
            assert(model::construction_error(graph, snapshot, packet, links@, error));
            return Err(error);
        }
    }
    assert(model::links_valid(links@, snapshot.spec_sections().len()));
    let profile = RoleProfile::for_harness_role(packet_role);
    assert(profile.spec_for_role(packet_role.spec_actor_role()));
    let entries = packet.entries();
    let mut selections = Vec::with_capacity(entries.len());
    let mut index = 0;
    while index < entries.len()
        invariant
            index <= entries.len(),
            selections.len() == index,
            packet_role == packet.spec_role(),
            entries@ == packet.spec_entries(),
            profile.spec_for_role(packet_role.spec_actor_role()),
            model::packet_binding_matches(snapshot, packet),
            model::links_valid(links@, snapshot.spec_sections().len()),
            model::first_entry_error(
                graph, snapshot, packet_role, links@, entries@, 0)
                == model::first_entry_error(
                    graph, snapshot, packet_role, links@, entries@, index as nat),
            forall |prior: int| #![auto] 0 <= prior < index ==>
                model::selection_refines_entry(
                    graph, snapshot, links@, entries@[prior], selections@[prior]),
        decreases entries.len() - index,
    {
        let entry = entries[index];
        match build::selection_for_entry(
            graph,
            snapshot,
            packet_role,
            &profile,
            links,
            entry,
        ) {
            Ok(selection) => selections.push(selection),
            Err(error) => {
                let ghost expected = model::entry_error(
                    graph,
                    snapshot,
                    packet_role,
                    links@,
                    entries@[index as int],
                ).unwrap();
                assert(model::first_entry_error(
                    graph, snapshot, packet_role, links@, entries@, index as nat)
                        == Some(expected)) by {
                    reveal(model::first_entry_error);
                }
                assert(model::first_entry_error(
                    graph, snapshot, packet.spec_role(), links@, packet.spec_entries(), 0)
                        == Some(expected));
                proof {
                    model::rejected_entry_is_exact(
                        graph, snapshot, packet, links@, expected, error);
                }
                return Err(error);
            }
        }
        index += 1;
    }
    assert(model::inputs_valid(graph, snapshot, packet, links@));
    Ok(selections)
}

} // verus!
