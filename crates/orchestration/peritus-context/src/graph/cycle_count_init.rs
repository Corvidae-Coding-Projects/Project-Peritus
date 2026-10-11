//! Edge-driven initialization of exact remaining-dependent counts.

use super::validation;
use crate::ContextNode;

pub(super) fn initial_counts(nodes: &[ContextNode]) -> Vec<usize> {
    let mut counts = Vec::with_capacity(nodes.len());
    while counts.len() < nodes.len() {
        counts.push(0usize);
    }
    let mut source = 0usize;
    while source < nodes.len() {
        let source_node = &nodes[source];
        let dependencies = source_node.dependencies();
        let mut dependency = 0usize;
        while dependency < dependencies.len() {
            let Some(target) = validation::find_node_index(nodes, dependencies[dependency]) else {
                return counts;
            };
            let next = counts[target] + 1;
            counts[target] = next;
            dependency += 1;
        }
        source += 1;
    }
    counts
}
