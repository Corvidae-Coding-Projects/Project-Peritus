//! Dependency validation and conservative transitive invalidation.

pub(super) use super::invalidation::invalidate_entries;

use super::{WorkingEntry, WorkingEntryStatus, WorkingError, WorkingState};
use crate::ContextNodeId;

pub(super) fn find_entry(entries: &[WorkingEntry], id: ContextNodeId) -> Option<usize> {
    let mut index = 0;
    while index < entries.len() {
        let candidate = entries[index].id();
        if candidate.matches(&id) {
            return Some(index);
        }
        index += 1;
    }
    None
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
    let mut removed = vec![false; entries.len()];
    let mut count = 0;
    while count < entries.len() {
        let mut candidate = None;
        index = 0;
        while index < entries.len() {
            if !removed[index] && incoming[index] == 0 {
                candidate = Some(index);
                break;
            }
            index += 1;
        }
        let Some(selected) = candidate else {
            return Err(WorkingError::DependencyCycle);
        };
        if selected >= entries.len() {
            return Err(WorkingError::DependencyCycle);
        }
        removed[selected] = true;
        count += 1;
        let edges = edges(&entries[selected]);
        let mut edge = 0;
        while edge < edges.len() {
            let Some(target) = find_entry(entries, edges[edge]) else {
                return Err(WorkingError::MissingEntry);
            };
            let Some(next) = incoming[target].checked_sub(1) else {
                return Err(WorkingError::DependencyCycle);
            };
            incoming[target] = next;
            edge += 1;
        }
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
