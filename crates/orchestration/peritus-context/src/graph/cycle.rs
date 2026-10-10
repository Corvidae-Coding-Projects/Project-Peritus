//! Production Kahn traversal connected to the finite cycle model.

use super::cycle_count_init;
use super::cycle_count_update;
use super::cycle_counts;

use crate::{ContextNode, ContextNodeId};

fn first_unremoved(removed: &[bool]) -> Option<usize> {
    let mut index = 0;
    while index < removed.len() {
        if !removed[index] {
            return Some(index);
        }
        index += 1;
    }
    None
}
fn initial_removal_state(node_count: usize) -> Vec<bool> {
    let mut removed: Vec<bool> = Vec::with_capacity(node_count);
    while removed.len() < node_count {
        removed.push(false);
    }
    removed
}
pub(super) fn cycle_member(nodes: &[ContextNode]) -> Option<ContextNodeId> {
    let mut removed = initial_removal_state(nodes.len());
    let mut dependent_counts = cycle_count_init::initial_counts(nodes);
    let mut removed_count = 0usize;
    let mut exhausted = false;
    while removed_count < nodes.len() && !exhausted {
        match cycle_counts::first_removable(nodes, removed.as_slice(), dependent_counts.as_slice())
        {
            None => {
                exhausted = true;
            }
            Some(index) => {
                cycle_count_update::remove_source(
                    nodes,
                    removed.as_slice(),
                    &mut dependent_counts,
                    index,
                );
                removed[index] = true;
                removed_count += 1;
            }
        }
    }
    let first = first_unremoved(removed.as_slice());
    if first.is_none() {
        return None;
    }
    let index = first.unwrap();
    Some(nodes[index].id())
}
