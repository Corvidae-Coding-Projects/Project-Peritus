//! Finite input and output model for run-knowledge context admission.

#[cfg(verus_only)]
use super::{KnowledgeContextLink, ReusableContextSelection};
#[cfg(verus_only)]
use crate::{ContextError, ContextErrorKind, ContextGraph, ContextNode, ContextNodeId, RoleVisibility};
#[cfg(verus_only)]
use core::cmp::Ordering;
#[cfg(verus_only)]
use peritus_role::{ContextClass, RoleProfile};
#[cfg(verus_only)]
use peritus_run_knowledge::{
    DeltaDelivery, DeltaEntry, DeltaPacket, KnowledgeAuthority, KnowledgeSection,
    KnowledgeSectionId, RunKnowledgeSnapshot,
};
use vstd::prelude::*;

verus! {

/// Exact snapshot/packet role and complete candidate identity binding.
pub open spec fn packet_binding_matches(
    snapshot: &RunKnowledgeSnapshot,
    packet: &DeltaPacket,
) -> bool {
    snapshot.spec_role() == packet.spec_role()
        && peritus_run_knowledge::model::candidates_match(
            &snapshot.spec_candidate(),
            &packet.spec_candidate(),
        )
}

/// First invalid canonical link pair at or after `index`.
pub open spec fn first_link_order_error(
    links: Seq<KnowledgeContextLink>,
    index: nat,
) -> Option<ContextErrorKind>
    decreases links.len() - index,
{
    if index >= links.len() {
        None
    } else {
        match links[index as int - 1]
            .spec_section_id()
            .spec_order(&links[index as int].spec_section_id())
        {
            Ordering::Less => first_link_order_error(links, index + 1),
            Ordering::Equal => Some(ContextErrorKind::DuplicateValue),
            Ordering::Greater => Some(ContextErrorKind::NonCanonicalOrder),
        }
    }
}

/// Canonical link admission under the exact snapshot-derived bound.
pub open spec fn links_valid(links: Seq<KnowledgeContextLink>, maximum: nat) -> bool {
    links.len() <= maximum && first_link_order_error(links, 1).is_none()
}

/// First exact section index at or after `index`.
pub open spec fn first_section_index_from(
    sections: Seq<KnowledgeSection>,
    id: KnowledgeSectionId,
    index: nat,
) -> Option<nat>
    decreases sections.len() - index,
{
    if index >= sections.len() {
        None
    } else if sections[index as int].spec_id().spec_matches(&id) {
        Some(index)
    } else {
        first_section_index_from(sections, id, index + 1)
    }
}

/// First exact link index at or after `index`.
pub open spec fn first_link_index_from(
    links: Seq<KnowledgeContextLink>,
    id: KnowledgeSectionId,
    index: nat,
) -> Option<nat>
    decreases links.len() - index,
{
    if index >= links.len() {
        None
    } else if links[index as int].spec_section_id().spec_matches(&id) {
        Some(index)
    } else {
        first_link_index_from(links, id, index + 1)
    }
}

/// Exact digest-byte agreement; SHA-256 authenticity remains outside this model.
pub open spec fn digest_bytes_match(section: &KnowledgeSection, node: &ContextNode) -> bool {
    section.spec_section_digest().spec_bytes() == node.spec_digest().spec_bytes()
}

/// Exact delivery class permitted by the section's fixed authority.
pub open spec fn delivery_matches_authority(
    delivery: DeltaDelivery,
    authority: KnowledgeAuthority,
) -> bool {
    match delivery {
        DeltaDelivery::ChangedFact | DeltaDelivery::CurrentReference => {
            authority == KnowledgeAuthority::Authoritative
        }
        DeltaDelivery::Navigation => authority == KnowledgeAuthority::NavigationOnly,
    }
}

/// Declarative role-policy visibility for one context class.
pub open spec fn role_class_visible(
    role: peritus_role::HarnessRole,
    class: ContextClass,
) -> bool {
    exists |profile: RoleProfile| profile.spec_for_role(role.spec_actor_role())
        && profile.spec_context().spec_visible().spec_values().contains(class)
}

/// Any constructed complete role profile decides the declarative visibility relation exactly.
pub proof fn profile_visibility_is_exact(
    profile: &RoleProfile,
    role: peritus_role::HarnessRole,
    class: ContextClass,
)
    requires profile.spec_for_role(role.spec_actor_role()),
    ensures
        profile.spec_context().spec_visible().spec_values().contains(class)
            == role_class_visible(role, class),
{
    assert forall |other: RoleProfile| other.spec_for_role(role.spec_actor_role()) implies
        #[trigger] other.spec_context().spec_visible().spec_values()
            == profile.spec_context().spec_visible().spec_values() by {
    }
}

/// Exact first admission error for one packet entry.
pub open spec fn entry_error(
    graph: &ContextGraph,
    snapshot: &RunKnowledgeSnapshot,
    role: peritus_role::HarnessRole,
    links: Seq<KnowledgeContextLink>,
    entry: DeltaEntry,
) -> Option<(ContextErrorKind, Option<ContextNodeId>)> {
    let section_index = first_section_index_from(
        snapshot.spec_sections(), entry.spec_section_id(), 0);
    if section_index.is_none() {
        Some((ContextErrorKind::KnowledgeSectionMissing, None))
    } else {
        let link_index = first_link_index_from(links, entry.spec_section_id(), 0);
        if link_index.is_none() {
            Some((ContextErrorKind::KnowledgeContextLinkMissing, None))
        } else {
            let link = links[link_index.unwrap() as int];
            let node_index = graph.spec_node_index(link.spec_node_id());
            if node_index.is_none() {
                Some((ContextErrorKind::PlanNodeMissing, Some(link.spec_node_id())))
            } else {
                let section = snapshot.spec_sections()[section_index.unwrap() as int];
                let node = graph.spec_nodes()[node_index.unwrap() as int];
                if !digest_bytes_match(&section, &node) {
                    Some((
                        ContextErrorKind::KnowledgeContextDigestMismatch,
                        Some(link.spec_node_id()),
                    ))
                } else if !RoleVisibility::roles_contain(
                    node.spec_visibility().spec_roles(),
                    role.spec_actor_role(),
                ) || !role_class_visible(role, node.spec_context_class())
                    || !delivery_matches_authority(
                        entry.spec_delivery(),
                        section.spec_kind().spec_authority(),
                    )
                {
                    Some((ContextErrorKind::KnowledgeRoleMismatch, Some(link.spec_node_id())))
                } else {
                    None
                }
            }
        }
    }
}

/// First packet-entry admission error in packet order.
pub open spec fn first_entry_error(
    graph: &ContextGraph,
    snapshot: &RunKnowledgeSnapshot,
    role: peritus_role::HarnessRole,
    links: Seq<KnowledgeContextLink>,
    entries: Seq<DeltaEntry>,
    index: nat,
) -> Option<(ContextErrorKind, Option<ContextNodeId>)>
    decreases entries.len() - index,
{
    if index >= entries.len() {
        None
    } else {
        match entry_error(graph, snapshot, role, links, entries[index as int]) {
            Some(error) => Some(error),
            None => first_entry_error(
                graph, snapshot, role, links, entries, index + 1),
        }
    }
}

/// Every deterministic input predicate checked by the production builder.
pub open spec fn inputs_valid(
    graph: &ContextGraph,
    snapshot: &RunKnowledgeSnapshot,
    packet: &DeltaPacket,
    links: Seq<KnowledgeContextLink>,
) -> bool {
    packet_binding_matches(snapshot, packet)
        && links_valid(links, snapshot.spec_sections().len())
        && first_entry_error(
            graph,
            snapshot,
            packet.spec_role(),
            links,
            packet.spec_entries(),
            0,
        ).is_none()
}

/// Exact public builder failure and branch priority.
pub open spec fn construction_error(
    graph: &ContextGraph,
    snapshot: &RunKnowledgeSnapshot,
    packet: &DeltaPacket,
    links: Seq<KnowledgeContextLink>,
    error: ContextError,
) -> bool {
    if !packet_binding_matches(snapshot, packet) {
        error.spec_is_plain(ContextErrorKind::KnowledgeRoleMismatch)
    } else if links.len() > snapshot.spec_sections().len() {
        error.spec_is_numbers(
            ContextErrorKind::TooManyNodes,
            snapshot.spec_sections().len() as u64,
            links.len() as u64,
        )
    } else if first_link_order_error(links, 1).is_some() {
        first_link_order_error(links, 1) == Some(error.spec_kind())
            && error.spec_node_id().is_none()
            && error.spec_related_id().is_none()
            && error.spec_expected().is_none()
            && error.spec_actual().is_none()
    } else {
        match first_entry_error(
            graph,
            snapshot,
            packet.spec_role(),
            links,
            packet.spec_entries(),
            0,
        ) {
            Some(expected) => error_matches(error, expected),
            None => false,
        }
    }
}

/// A concrete first rejected entry establishes exact overall failure.
pub proof fn rejected_entry_is_exact(
    graph: &ContextGraph,
    snapshot: &RunKnowledgeSnapshot,
    packet: &DeltaPacket,
    links: Seq<KnowledgeContextLink>,
    expected: (ContextErrorKind, Option<ContextNodeId>),
    error: ContextError,
)
    requires
        packet_binding_matches(snapshot, packet),
        links_valid(links, snapshot.spec_sections().len()),
        first_entry_error(
            graph,
            snapshot,
            packet.spec_role(),
            links,
            packet.spec_entries(),
            0,
        ) == Some(expected),
        error_matches(error, expected),
    ensures
        !inputs_valid(graph, snapshot, packet, links),
        construction_error(graph, snapshot, packet, links, error),
{
}

/// Exact error payload for one optional node identity.
pub open spec fn error_matches(
    error: ContextError,
    expected: (ContextErrorKind, Option<ContextNodeId>),
) -> bool {
    match expected.1 {
        Some(id) => error.spec_is_node(expected.0, id),
        None => error.spec_is_plain(expected.0),
    }
}

/// One output selection exactly refines its packet entry and resolved inputs.
pub open spec fn selection_refines_entry(
    graph: &ContextGraph,
    snapshot: &RunKnowledgeSnapshot,
    links: Seq<KnowledgeContextLink>,
    entry: DeltaEntry,
    selection: ReusableContextSelection,
) -> bool {
    let section_index = first_section_index_from(
        snapshot.spec_sections(), entry.spec_section_id(), 0);
    let link_index = first_link_index_from(links, entry.spec_section_id(), 0);
    section_index.is_some() && link_index.is_some() && {
        let section = snapshot.spec_sections()[section_index.unwrap() as int];
        let link = links[link_index.unwrap() as int];
        graph.spec_node_index(link.spec_node_id()).is_some()
            && selection.spec_node_id() == link.spec_node_id()
            && KnowledgeSection::clone_equivalent(&section, &selection.spec_section())
            && selection.spec_delivery() == entry.spec_delivery()
    }
}

/// Complete packet-order output relation.
pub open spec fn selections_refine(
    graph: &ContextGraph,
    snapshot: &RunKnowledgeSnapshot,
    links: Seq<KnowledgeContextLink>,
    entries: Seq<DeltaEntry>,
    selections: Seq<ReusableContextSelection>,
) -> bool {
    selections.len() == entries.len()
        && forall |index: int| #![auto] 0 <= index < entries.len() ==>
            selection_refines_entry(
                graph, snapshot, links, entries[index], selections[index])
}

} // verus!
