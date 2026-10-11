//! Exact graph construction after source validation.

use super::dependencies::{contains, external_dependencies, rewrite_dependencies};

use super::super::ValidatedCompaction;
use crate::{ContextError, ContextGraph, ContextNodeId};

#[allow(clippy::too_many_lines, reason = "the graph rewrite and its proof are one transaction")]
pub(super) fn build_replacement_graph(
    graph: &ContextGraph,
    validated: &ValidatedCompaction,
    source_ids: &[ContextNodeId],
) -> Result<ContextGraph, ContextError> {
    let output_id = validated.node.id();
    let live_dependencies = external_dependencies(validated, source_ids);
    let replacement = validated.node.replace_dependencies(live_dependencies, graph.limits())?;
    let graph_nodes = graph.nodes();
    let mut nodes = Vec::with_capacity(graph_nodes.len());
    let mut replacement_inserted = false;
    let mut node_index = 0;
    while node_index < graph_nodes.len() {
        let node = &graph_nodes[node_index];
        if !replacement_inserted && output_id < node.id() {
            let inserted_replacement = replacement.clone();
            nodes.push(inserted_replacement);
            replacement_inserted = true;
        }
        let is_source = contains(source_ids, node.id());
        if !is_source {
            let dependencies = rewrite_dependencies(node.dependencies(), source_ids, output_id);
            let rewritten_node = node.replace_dependencies(dependencies, graph.limits())?;
            nodes.push(rewritten_node);
        }
        node_index += 1;
    }
    if !replacement_inserted {
        nodes.push(replacement);
    }
    let replacement_graph = ContextGraph::new(nodes, graph.limits())?;
    Ok(replacement_graph)
}
