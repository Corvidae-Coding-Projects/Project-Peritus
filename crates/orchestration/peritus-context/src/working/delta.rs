//! Revision-checked, all-or-nothing working-model updates.

use super::state::next_revision;
use super::validation::{find_entry, invalidate_entries, unusable, validate_graph, validate_references};
use super::{WorkingBinding, WorkingEntry, WorkingEntryStatus, WorkingError, WorkingLimits, WorkingState};
use vstd::prelude::*;

verus! {
/// An ordered bounded set of source-backed upserts against one exact state revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkingDelta {
    binding: WorkingBinding,
    base_revision: u64,
    entries: Vec<WorkingEntry>,
}

impl WorkingDelta {
    /// Creates an atomic proposal. Rejections do not partially update the model.
    ///
    /// # Errors
    /// Rejects empty, excessive, unordered, duplicate, or host-status-bearing proposals.
    pub fn new(
        binding: WorkingBinding,
        base_revision: u64,
        entries: Vec<WorkingEntry>,
        limits: WorkingLimits,
    ) -> Result<Self, WorkingError> {
        if entries.is_empty() { return Err(WorkingError::EmptyEntry); }
        if entries.len() > limits.operations() { return Err(WorkingError::Capacity); }
        let mut index = 0;
        while index < entries.len()
            invariant index <= entries.len(),
            decreases entries.len() - index,
        {
            if unusable(entries[index].status()) { return Err(WorkingError::DerivedStatus); }
            if index > 0 && entries[index - 1].id() >= entries[index].id() {
                return Err(WorkingError::NonCanonicalOrder);
            }
            index += 1;
        }
        Ok(Self { binding, base_revision, entries })
    }
    /// Target task/role/conversation binding.
    #[must_use]
    pub const fn binding(&self) -> WorkingBinding { self.binding }
    /// Exact prior state revision.
    #[must_use]
    pub const fn base_revision(&self) -> u64 { self.base_revision }
    /// Source-backed proposals in canonical entry order.
    #[must_use]
    pub const fn entries(&self) -> &[WorkingEntry] { self.entries.as_slice() }
}

/// Applies a complete source-checked proposal and validates the resulting dependency graph.
///
/// Superseded entries and their source references remain retained. Stale entries require fresh
/// supporting or contradicting observations before they can be replaced with current claims.
///
/// # Errors
/// Rejects wrong scope/revision, missing sources or dependencies, cycles, stale proposals,
/// resurrection of superseded records, allocation overflow, or exhausted revisions.
pub fn apply_working_delta(state: &WorkingState, delta: &WorkingDelta) -> Result<WorkingState, WorkingError> {
    state.check_binding(delta.binding)?;
    if delta.base_revision != state.revision { return Err(WorkingError::RevisionMismatch); }
    if delta.entries.len() > state.limits.operations() { return Err(WorkingError::Capacity); }
    let revision = next_revision(state.revision)?;
    let mut next = state.clone();
    let mut index = 0;
    while index < delta.entries.len()
        invariant index <= delta.entries.len(),
        decreases delta.entries.len() - index,
    {
        let entry = &delta.entries[index];
        validate_references(state, entry)?;
        validate_upsert(state, entry)?;
        let checked = WorkingEntry::new(entry.id, entry.kind, entry.content.clone(), entry.links.clone(), entry.validity.clone(), state.limits)?;
        if let Some(position) = find_entry(&next.entries, checked.id()) {
            next.entries[position] = entry.clone();
        } else {
            if next.entries.len() >= state.limits.entries() { return Err(WorkingError::Capacity); }
            insert_entry(&mut next.entries, entry.clone());
        }
        index += 1;
    }
    validate_graph(&next.entries)?;
    index = 0;
    while index < delta.entries.len()
        invariant index <= delta.entries.len(),
        decreases delta.entries.len() - index,
    {
        let entry = &delta.entries[index];
        if let Some(previous) = entry.supersedes {
            let Some(target) = find_entry(&next.entries, previous) else { return Err(WorkingError::MissingEntry); };
            if next.entries[target].status == WorkingEntryStatus::Superseded {
                let Some(original) = find_entry(&state.entries, entry.id) else { return Err(WorkingError::AlreadySuperseded); };
                if state.entries[original].supersedes != Some(previous) { return Err(WorkingError::AlreadySuperseded); }
            }
            next.entries[target].status = WorkingEntryStatus::Superseded;
        }
        index += 1;
    }
    next.entries = invalidate_entries(&next.entries, &next.environment, state.through_observation());
    index = 0;
    while index < delta.entries.len()
        invariant index <= delta.entries.len(),
        decreases delta.entries.len() - index,
    {
        let Some(target) = find_entry(&next.entries, delta.entries[index].id) else { return Err(WorkingError::MissingEntry); };
        if next.entries[target].status == WorkingEntryStatus::Stale { return Err(WorkingError::StaleEntry); }
        index += 1;
    }
    next.revision = revision;
    Ok(next)
}

fn validate_upsert(state: &WorkingState, entry: &WorkingEntry) -> Result<(), WorkingError> {
    if let Some(previous) = find_entry(&state.entries, entry.id) {
        let old = &state.entries[previous];
        if old.status == WorkingEntryStatus::Superseded { return Err(WorkingError::AlreadySuperseded); }
        if old.supersedes.is_some() && old.supersedes != entry.supersedes {
            return Err(WorkingError::AlreadySuperseded);
        }
        if old.status == WorkingEntryStatus::Stale && !has_new_source(old, entry) {
            return Err(WorkingError::StaleEntry);
        }
    }
    Ok(())
}

fn has_new_source(old: &WorkingEntry, new: &WorkingEntry) -> bool {
    let newest = old.stale_through;
    let supports = new.links.supports();
    let contradicts = new.links.contradicts();
    let mut index = 0;
    while index < supports.len()
        invariant index <= supports.len(),
        decreases supports.len() - index,
    {
        if supports[index].get() > newest { return true; }
        index += 1;
    }
    index = 0;
    while index < contradicts.len()
        invariant index <= contradicts.len(),
        decreases contradicts.len() - index,
    {
        if contradicts[index].get() > newest { return true; }
        index += 1;
    }
    false
}

fn insert_entry(entries: &mut Vec<WorkingEntry>, entry: WorkingEntry) {
    let mut position = 0;
    while position < entries.len()
        invariant position <= entries.len(),
        decreases entries.len() - position,
    {
        if entries[position].id() > entry.id() { break; }
        position += 1;
    }
    entries.insert(position, entry);
}
}
