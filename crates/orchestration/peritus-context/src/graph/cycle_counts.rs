//! Exact zero-count selection for the production Kahn traversal.

use crate::ContextNode;

pub(super) fn first_removable(
    nodes: &[ContextNode],
    removed: &[bool],
    counts: &[usize],
) -> Option<usize> {
    let mut index = 0usize;
    while index < nodes.len() {
        if !removed[index] && counts[index] == 0 {
            return Some(index);
        }
        index += 1;
    }
    None
}
