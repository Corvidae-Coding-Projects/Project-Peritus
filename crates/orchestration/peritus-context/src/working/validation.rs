//! Dependency validation and conservative transitive invalidation.

use super::{WorkingEntry, WorkingEntryStatus, WorkingEnvironment, WorkingError, WorkingState};
use crate::ContextNodeId;
use vstd::prelude::*;

verus! {
pub(super) fn find_entry(entries: &[WorkingEntry], id: ContextNodeId) -> (found: Option<usize>)
    ensures match found { Some(index) => index < entries.len(), None => true },
{
    let mut index = 0;
    while index < entries.len()
        invariant index <= entries.len(),
        decreases entries.len() - index,
    {
        if entries[index].id() == id { return Some(index); }
        index += 1;
    }
    None
}

pub(super) fn validate_references(state: &WorkingState, entry: &WorkingEntry) -> Result<(), WorkingError> {
    let links = entry.links();
    let supports = links.supports();
    let contradicts = links.contradicts();
    let mut index = 0;
    while index < supports.len()
        invariant index <= supports.len(),
        decreases supports.len() - index,
    {
        state.observation(state.binding(), supports[index])?;
        index += 1;
    }
    index = 0;
    while index < contradicts.len()
        invariant index <= contradicts.len(),
        decreases contradicts.len() - index,
    {
        state.observation(state.binding(), contradicts[index])?;
        index += 1;
    }
    Ok(())
}

/// Kahn elimination over dependency and supersession edges. No recursion or unbounded stack.
pub(super) fn validate_graph(entries: &[WorkingEntry]) -> Result<(), WorkingError> {
    let mut incoming = vec![0usize; entries.len()];
    let mut index = 0;
    while index < entries.len()
        invariant index <= entries.len(), incoming.len() == entries.len(),
        decreases entries.len() - index,
    {
        let edges = edges(&entries[index]);
        let mut edge = 0;
        while edge < edges.len()
            invariant edge <= edges.len(), incoming.len() == entries.len(),
            decreases edges.len() - edge,
        {
            let Some(target) = find_entry(entries, edges[edge]) else { return Err(WorkingError::MissingEntry); };
            let Some(count) = incoming[target].checked_add(1) else { return Err(WorkingError::Capacity); };
            incoming[target] = count;
            edge += 1;
        }
        index += 1;
    }
    let mut removed = vec![false; entries.len()];
    let mut count = 0;
    while count < entries.len()
        invariant count <= entries.len(), incoming.len() == entries.len(), removed.len() == entries.len(),
        decreases entries.len() - count,
    {
        let mut candidate = None;
        index = 0;
        while index < entries.len()
            invariant index <= entries.len(), incoming.len() == entries.len(), removed.len() == entries.len(),
            decreases entries.len() - index,
        {
            if !removed[index] && incoming[index] == 0 { candidate = Some(index); break; }
            index += 1;
        }
        let Some(selected) = candidate else { return Err(WorkingError::DependencyCycle); };
        if selected >= entries.len() { return Err(WorkingError::DependencyCycle); }
        removed[selected] = true;
        count += 1;
        let edges = edges(&entries[selected]);
        let mut edge = 0;
        while edge < edges.len()
            invariant edge <= edges.len(), incoming.len() == entries.len(),
            decreases edges.len() - edge,
        {
            let Some(target) = find_entry(entries, edges[edge]) else { return Err(WorkingError::MissingEntry); };
            let Some(next) = incoming[target].checked_sub(1) else { return Err(WorkingError::DependencyCycle); };
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
    while index < dependencies.len()
        invariant index <= dependencies.len(),
        decreases dependencies.len() - index,
    {
        edges.push(dependencies[index]);
        index += 1;
    }
    if let Some(previous) = entry.supersedes { edges.push(previous); }
    edges
}

pub(super) fn invalidate_entries(entries: &[WorkingEntry], environment: &WorkingEnvironment, through: u64) -> Vec<WorkingEntry> {
    let mut result = Vec::new();
    let mut position = 0;
    while position < entries.len()
        invariant position <= entries.len(), result.len() == position,
        decreases entries.len() - position,
    {
        result.push(entries[position].clone());
        position += 1;
    }
    let mut pass = 0;
    while pass < entries.len()
        invariant pass <= entries.len(), result.len() == entries.len(),
        decreases entries.len() - pass,
    {
        let mut changed = false;
        let mut index = 0;
        while index < result.len()
            invariant index <= result.len(), result.len() == entries.len(),
            decreases result.len() - index,
        {
            if result[index].status == WorkingEntryStatus::Stale
                && !result[index].validity.holds(environment)
            {
                result[index].stale_through = through;
            }
            if !unusable(result[index].status)
                && (!result[index].validity.holds(environment) || stale_dependency(&result, &result[index]))
            {
                result[index].status = WorkingEntryStatus::Stale;
                result[index].stale_through = through;
                changed = true;
            }
            index += 1;
        }
        if !changed { break; }
        pass += 1;
    }
    result
}

fn stale_dependency(entries: &[WorkingEntry], entry: &WorkingEntry) -> bool {
    let dependencies = entry.links.depends_on();
    let mut index = 0;
    while index < dependencies.len()
        invariant index <= dependencies.len(),
        decreases dependencies.len() - index,
    {
        let Some(target) = find_entry(entries, dependencies[index]) else { return true; };
        if unusable(entries[target].status) { return true; }
        index += 1;
    }
    false
}

pub(super) const fn unusable(status: WorkingEntryStatus) -> bool {
    matches!(status, WorkingEntryStatus::Stale | WorkingEntryStatus::Superseded)
}
}
