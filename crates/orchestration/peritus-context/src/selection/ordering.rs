//! Deterministic optional-root ranking and final render ordering.

use super::closure::is_visible;
use crate::{ContextGraph, RequirementMode, SelectedContext};
use peritus_role::RoleProfile;

pub(super) fn ranked_optional_roots(
    graph: &ContextGraph,
    selected: &[bool],
    role: &RoleProfile,
) -> Vec<usize> {
    let graph_nodes = graph.nodes();
    let graph_len = graph_nodes.len();
    let mut ranked = Vec::new();
    if selected.len() != graph_len {
        return ranked;
    }
    let mut index = 0;
    while index < graph_len {
        let node = &graph_nodes[index];
        if node.requirement() == RequirementMode::Optional
            && !selected[index]
            && is_visible(node, role)
        {
            let mut position = ranked.len();
            while position > 0 {
                let previous_index = ranked[position - 1];
                if previous_index >= graph_nodes.len() {
                    return Vec::new();
                }
                if !crate::precedence::optional_precedes(node, &graph_nodes[previous_index]) {
                    break;
                }
                position -= 1;
            }
            ranked.insert(position, index);
        }
        index += 1;
    }
    ranked
}
pub(super) fn sort_for_render(graph: &ContextGraph, entries: &mut Vec<SelectedContext>) {
    let entries_len = entries.len();
    if entries_len < 2 {
        return;
    }
    let mut index = 1;
    while index < entries_len {
        let entry = entries[index];
        let Some(node) = graph.node(entry.node_id()) else { return };
        let mut position = index;
        while position > 0 {
            let Some(previous) = graph.node(entries[position - 1].node_id()) else { return };
            if !crate::precedence::render_precedes(node, previous) {
                break;
            }
            position -= 1;
        }
        if position != index {
            entries.remove(index);
            entries.insert(position, entry);
        }
        index += 1;
    }
}
