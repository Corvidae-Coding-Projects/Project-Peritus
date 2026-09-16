//! One-entry production admission for reusable context selections.

#[cfg(verus_only)]
use super::model;
use super::{KnowledgeContextLink, ReusableContextSelection, validation};
use crate::{ContextError, ContextErrorKind, ContextGraph};
use peritus_role::RoleProfile;
use peritus_run_knowledge::{DeltaEntry, RunKnowledgeSnapshot};
use vstd::prelude::*;

verus! {

pub(super) fn selection_for_entry(
    graph: &ContextGraph,
    snapshot: &RunKnowledgeSnapshot,
    role: peritus_role::HarnessRole,
    profile: &RoleProfile,
    links: &[KnowledgeContextLink],
    entry: DeltaEntry,
) -> (result: Result<ReusableContextSelection, ContextError>)
    requires profile.spec_for_role(role.spec_actor_role()),
    ensures
        result.is_ok() == model::entry_error(graph, snapshot, role, links@, entry).is_none(),
        match result {
            Ok(selection) => model::selection_refines_entry(
                graph, snapshot, links@, entry, selection),
            Err(error) => match model::entry_error(graph, snapshot, role, links@, entry) {
                Some(expected) => model::error_matches(error, expected),
                None => false,
            },
        },
{
    let Some(section_index) = validation::find_section_index(
        snapshot.sections(), entry.section_id()) else {
        return Err(ContextError::plain(ContextErrorKind::KnowledgeSectionMissing));
    };
    let section = &snapshot.sections()[section_index];
    let Some(link_index) = validation::find_link_index(links, entry.section_id()) else {
        return Err(ContextError::plain(ContextErrorKind::KnowledgeContextLinkMissing));
    };
    let link = links[link_index];
    let Some(node_index) = graph.index_of(link.node_id()) else {
        return Err(ContextError::node(ContextErrorKind::PlanNodeMissing, link.node_id()));
    };
    let node = &graph.nodes()[node_index];
    if !validation::digest_bytes_match(&node.digest(), &section.section_digest()) {
        return Err(ContextError::node(
            ContextErrorKind::KnowledgeContextDigestMismatch,
            link.node_id(),
        ));
    }
    let role_visible = node.visibility().contains(role.actor_role());
    let class_visible = profile.context().visible().contains(node.context_class());
    proof {
        model::profile_visibility_is_exact(profile, role, node.spec_context_class());
    }
    if !role_visible
        || !class_visible
        || !validation::delivery_matches_authority(entry.delivery(), section.authority())
    {
        return Err(ContextError::node(
            ContextErrorKind::KnowledgeRoleMismatch,
            link.node_id(),
        ));
    }
    Ok(ReusableContextSelection {
        node_id: link.node_id(),
        section: section.clone(),
        delivery: entry.delivery(),
    })
}

} // verus!
