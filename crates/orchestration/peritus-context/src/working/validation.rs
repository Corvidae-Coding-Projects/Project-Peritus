//! Dependency validation and conservative transitive invalidation.

pub(super) use super::invalidation::invalidate_entries;
#[cfg(verus_only)]
pub(super) use super::invalidation_model::{spec_invalidation_result, spec_status_view, spec_unusable};
use super::{WorkingEntry, WorkingEntryStatus, WorkingError, WorkingState};
use crate::ContextNodeId;
use vstd::prelude::*;

verus! {
pub(super) open spec fn spec_find_entry_from(
    entries: Seq<WorkingEntry>,
    id: ContextNodeId,
    start: int,
) -> Option<int>
    decreases entries.len() - start,
{
    if start < 0 || start >= entries.len() {
        None
    } else if entries[start].spec_id().spec_matches(&id) {
        Some(start)
    } else {
        spec_find_entry_from(entries, id, start + 1)
    }
}

pub(super) fn find_entry(entries: &[WorkingEntry], id: ContextNodeId) -> (found: Option<usize>)
    ensures match found {
        Some(index) => index < entries.len()
            && Some(index as int) == spec_find_entry_from(entries@, id, 0),
        None => spec_find_entry_from(entries@, id, 0) == None,
    },
{
    let mut index = 0;
    while index < entries.len()
        invariant
            index <= entries.len(),
            spec_find_entry_from(entries@, id, index as int)
                == spec_find_entry_from(entries@, id, 0),
        decreases entries.len() - index,
    {
        let candidate = entries[index].id();
        if candidate.matches(&id) {
            reveal_with_fuel(spec_find_entry_from, 1);
            assert(candidate == entries@[index as int].spec_id());
            assert(spec_find_entry_from(entries@, id, index as int) == Some(index as int));
            return Some(index);
        }
        reveal_with_fuel(spec_find_entry_from, 1);
        assert(candidate == entries@[index as int].spec_id());
        assert(spec_find_entry_from(entries@, id, index as int)
            == spec_find_entry_from(entries@, id, index as int + 1));
        index += 1;
    }
    reveal_with_fuel(spec_find_entry_from, 1);
    assert(spec_find_entry_from(entries@, id, index as int).is_none());
    None
}

pub(super) fn validate_references(
    state: &WorkingState,
    entry: &WorkingEntry,
) -> (result: Result<(), WorkingError>)
    ensures result.is_err() ==> result.unwrap_err().spec_is_delta_error(),
{
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
pub(super) fn validate_graph(entries: &[WorkingEntry]) -> (result: Result<(), WorkingError>)
    ensures result.is_err() ==> result.unwrap_err().spec_is_delta_error(),
{
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

pub(super) const fn unusable(status: WorkingEntryStatus) -> (unusable: bool)
    ensures unusable == spec_unusable(status),
{
    matches!(status, WorkingEntryStatus::Stale | WorkingEntryStatus::Superseded)
}

}
