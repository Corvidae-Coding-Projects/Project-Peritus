//! Deterministic entry upsert and supersession helpers.

use super::validation::find_entry;
use super::{WorkingEntry, WorkingEntryStatus, WorkingError, WorkingState};

pub(super) fn upsert_entry(
    entries: &mut Vec<WorkingEntry>,
    entry: &WorkingEntry,
    maximum: usize,
) -> Result<(), WorkingError> {
    if let Some(position) = find_entry(entries, entry.id()) {
        let replacement = entry.clone();
        entries[position] = replacement;
        return Ok(());
    }
    if entries.len() >= maximum {
        return Err(WorkingError::Capacity);
    }
    insert_entry(entries, entry);
    Ok(())
}
pub(super) fn apply_supersession(
    entries: &mut [WorkingEntry],
    original_entries: &[WorkingEntry],
    entry: &WorkingEntry,
) -> Result<(), WorkingError> {
    let Some(previous) = entry.supersedes() else {
        return Ok(());
    };
    let Some(target) = find_entry(entries, previous) else {
        return Err(WorkingError::MissingEntry);
    };
    if entries[target].status == WorkingEntryStatus::Superseded {
        let Some(original) = find_entry(original_entries, entry.id) else {
            return Err(WorkingError::AlreadySuperseded);
        };
        if original_entries[original].supersedes != Some(previous) {
            return Err(WorkingError::AlreadySuperseded);
        }
    }
    let stale_through = entries[target].stale_through();
    let replacement =
        entries[target].clone().with_host_status(WorkingEntryStatus::Superseded, stale_through);
    entries[target] = replacement;
    Ok(())
}
pub(super) fn validate_upsert(
    state: &WorkingState,
    entry: &WorkingEntry,
) -> Result<(), WorkingError> {
    if let Some(previous) = find_entry(&state.entries, entry.id) {
        let old = &state.entries[previous];
        if old.status == WorkingEntryStatus::Superseded {
            return Err(WorkingError::AlreadySuperseded);
        }
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
    while index < supports.len() {
        if supports[index].get() > newest {
            return true;
        }
        index += 1;
    }
    index = 0;
    while index < contradicts.len() {
        if contradicts[index].get() > newest {
            return true;
        }
        index += 1;
    }
    false
}
fn insert_entry(entries: &mut Vec<WorkingEntry>, entry: &WorkingEntry) {
    let mut position = 0;
    let mut found_greater = false;
    while position < entries.len() && !found_greater {
        let current_id = entries[position].id();
        let proposed_id = entry.id();
        let order = current_id.canonical_order(&proposed_id);
        match order {
            core::cmp::Ordering::Greater => {
                found_greater = true;
            }
            core::cmp::Ordering::Equal | core::cmp::Ordering::Less => {
                position += 1;
            }
        }
    }
    let replacement = entry.clone();
    entries.insert(position, replacement);
}
