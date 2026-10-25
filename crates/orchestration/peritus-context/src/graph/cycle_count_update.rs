//! Edge-driven maintenance of exact remaining-dependent counts.

use super::validation;
use crate::ContextNode;

pub(super) fn remove_source(
    nodes: &[ContextNode],
    _removed: &[bool],
    counts: &mut [usize],
    source: usize,
) {
    let source_node = &nodes[source];
    let dependencies = source_node.dependencies();
    let mut dependency = 0usize;
    while dependency < dependencies.len() {
        let Some(target) = validation::find_node_index(nodes, dependencies[dependency]) else {
            return;
        };
        let next = counts[target] - 1;
        counts[target] = next;
        dependency += 1;
    }
}
