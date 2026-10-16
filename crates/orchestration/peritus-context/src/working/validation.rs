//! Dependency validation and conservative transitive invalidation.

pub(super) use super::invalidation::invalidate_entries;

use super::{WorkingEntry, WorkingEntryStatus, WorkingError, WorkingState};
use crate::ContextNodeId;

pub(super) fn find_entry(entries: &[WorkingEntry], id: ContextNodeId) -> Option<usize> {
    entries.binary_search_by_key(&id, WorkingEntry::id).ok()
}
pub(super) fn validate_references(
    state: &WorkingState,
    entry: &WorkingEntry,
) -> Result<(), WorkingError> {
    let links = entry.links();
    let supports = links.supports();
    let contradicts = links.contradicts();
    let mut index = 0;
    while index < supports.len() {
        state.observation(state.binding(), supports[index])?;
        index += 1;
    }
    index = 0;
    while index < contradicts.len() {
        state.observation(state.binding(), contradicts[index])?;
        index += 1;
    }
    Ok(())
}
#[doc = " Kahn elimination over dependency and supersession edges. No recursion or unbounded stack."]
pub(super) fn validate_graph(entries: &[WorkingEntry]) -> Result<(), WorkingError> {
    let mut incoming = vec![0usize; entries.len()];
    let mut index = 0;
    while index < entries.len() {
        let edges = edges(&entries[index]);
        let mut edge = 0;
        while edge < edges.len() {
            let Some(target) = find_entry(entries, edges[edge]) else {
                return Err(WorkingError::MissingEntry);
            };
            let Some(count) = incoming[target].checked_add(1) else {
                return Err(WorkingError::Capacity);
            };
            incoming[target] = count;
            edge += 1;
        }
        index += 1;
    }
    let mut ready = std::collections::VecDeque::new();
    for (index, count) in incoming.iter().enumerate() {
        if *count == 0 {
            ready.push_back(index);
        }
    }
    let mut count = 0;
    while let Some(selected) = ready.pop_front() {
        count += 1;
        for edge in edges(&entries[selected]) {
            let target = find_entry(entries, edge).ok_or(WorkingError::MissingEntry)?;
            incoming[target] =
                incoming[target].checked_sub(1).ok_or(WorkingError::DependencyCycle)?;
            if incoming[target] == 0 {
                ready.push_back(target);
            }
        }
    }
    if count != entries.len() {
        return Err(WorkingError::DependencyCycle);
    }
    Ok(())
}
fn edges(entry: &WorkingEntry) -> Vec<ContextNodeId> {
    let dependencies = entry.links.depends_on();
    let mut edges = Vec::new();
    let mut index = 0;
    while index < dependencies.len() {
        edges.push(dependencies[index]);
        index += 1;
    }
    if let Some(previous) = entry.supersedes {
        edges.push(previous);
    }
    edges
}
pub(super) const fn unusable(status: WorkingEntryStatus) -> bool {
    matches!(status, WorkingEntryStatus::Stale | WorkingEntryStatus::Superseded)
}
