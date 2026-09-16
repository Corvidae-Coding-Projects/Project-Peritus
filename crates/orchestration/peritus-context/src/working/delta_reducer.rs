//! Deterministic entry upsert and supersession helpers.

#[cfg(verus_only)]
use super::delta_model::{spec_one_supersession, spec_one_upsert};
#[cfg(verus_only)]
use super::validation::spec_find_entry_from;
use super::validation::find_entry;
use super::{WorkingEntry, WorkingEntryStatus, WorkingError, WorkingState};
use vstd::prelude::*;

verus! {
pub(super) fn upsert_entry(
    entries: &mut Vec<WorkingEntry>,
    entry: &WorkingEntry,
    maximum: usize,
) -> (result: Result<(), WorkingError>)
    ensures
        result.is_ok() ==> spec_one_upsert(old(entries)@, entry, final(entries)@),
        result.is_err() ==> result.unwrap_err().spec_is_delta_error(),
{
    let ghost before = entries@;
    if let Some(position) = find_entry(entries, entry.id()) {
        let replacement = entry.clone();
        entries[position] = replacement;
        proof {
            reveal(spec_one_upsert);
            assert forall |index: int| #![auto]
                0 <= index < before.len() && index != position as int implies
                WorkingEntry::clone_equivalent(&before[index], &entries@[index]) by {
                WorkingEntry::clone_reflexive(&before[index]);
            }
        }
        return Ok(());
    }
    if entries.len() >= maximum { return Err(WorkingError::Capacity); }
    insert_entry(entries, entry);
    Ok(())
}

pub(super) fn apply_supersession(
    entries: &mut [WorkingEntry],
    original_entries: &[WorkingEntry],
    entry: &WorkingEntry,
) -> (result: Result<(), WorkingError>)
    ensures
        result.is_ok() ==> spec_one_supersession(old(entries)@, entry, final(entries)@),
        result.is_err() ==> result.unwrap_err().spec_is_delta_error(),
{
    let ghost before = entries@;
    let Some(previous) = entry.supersedes() else {
        proof {
            reveal(spec_one_supersession);
            assert(WorkingEntry::sequence_clone_equivalent(before, entries@)) by {
                reveal(WorkingEntry::sequence_clone_equivalent);
                assert forall |index: int| #![auto] 0 <= index < before.len() implies
                    WorkingEntry::clone_equivalent(&before[index], &entries@[index]) by {
                    WorkingEntry::clone_reflexive(&before[index]);
                }
            }
        }
        return Ok(());
    };
    let Some(target) = find_entry(entries, previous) else { return Err(WorkingError::MissingEntry); };
    if entries[target].status == WorkingEntryStatus::Superseded {
        let Some(original) = find_entry(original_entries, entry.id) else { return Err(WorkingError::AlreadySuperseded); };
        if original_entries[original].supersedes != Some(previous) {
            return Err(WorkingError::AlreadySuperseded);
        }
    }
    let stale_through = entries[target].stale_through();
    let replacement = entries[target].clone().with_host_status(
        WorkingEntryStatus::Superseded,
        stale_through,
    );
    entries[target] = replacement;
    proof {
        reveal(spec_one_supersession);
        assert(spec_find_entry_from(before, previous, 0)
            == Some(target as int));
        assert(WorkingEntry::payload_equivalent(
            &before[target as int],
            &entries@[target as int],
        ));
        assert(entries@[target as int].spec_status() == WorkingEntryStatus::Superseded);
        assert(entries@[target as int].spec_stale_through()
            == before[target as int].spec_stale_through());
        assert forall |index: int| #![auto]
            0 <= index < before.len() && index != target as int implies
            WorkingEntry::clone_equivalent(&before[index], &entries@[index]) by {
            WorkingEntry::clone_reflexive(&before[index]);
        }
    }
    Ok(())
}

pub(super) fn validate_upsert(
    state: &WorkingState,
    entry: &WorkingEntry,
) -> (result: Result<(), WorkingError>)
    ensures result.is_err() ==> result.unwrap_err().spec_is_delta_error(),
{
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

fn insert_entry(entries: &mut Vec<WorkingEntry>, entry: &WorkingEntry)
    requires spec_find_entry_from(old(entries)@, entry.spec_id(), 0) == None,
    ensures spec_one_upsert(old(entries)@, entry, final(entries)@),
{
    let ghost before = entries@;
    let mut position = 0;
    let mut found_greater = false;
    while position < entries.len() && !found_greater
        invariant
            position <= entries.len(),
            entries@ == before,
            found_greater ==> position < before.len()
                && before[position as int].spec_id().spec_order(&entry.spec_id())
                    == core::cmp::Ordering::Greater,
            forall |index: int| #![auto] 0 <= index < position ==>
                before[index].spec_id().spec_order(&entry.spec_id())
                    != core::cmp::Ordering::Greater,
        decreases entries.len() - position, if found_greater { 0nat } else { 1nat },
    {
        let current_id = entries[position].id();
        let proposed_id = entry.id();
        let order = current_id.canonical_order(&proposed_id);
        proof {
            assert(current_id == before[position as int].spec_id());
            assert(proposed_id == entry.spec_id());
            assert(order == before[position as int].spec_id().spec_order(&entry.spec_id()));
        }
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
    proof {
        reveal(spec_one_upsert);
        assert(entries@ == before.insert(position as int, replacement));
        assert(0 <= position as int <= before.len());
        assert(entries@.len() == before.len() + 1);
        assert(entries@[position as int] == replacement);
        assert(WorkingEntry::clone_equivalent(entry, &entries@[position as int]));
        assert(position == before.len() || found_greater);
        assert(position == before.len()
            || before[position as int].spec_id().spec_order(&entry.spec_id())
                == core::cmp::Ordering::Greater);
        assert forall |index: int| #![auto] 0 <= index < position implies
            WorkingEntry::clone_equivalent(&before[index], &entries@[index]) by {
            WorkingEntry::clone_reflexive(&before[index]);
        }
        assert forall |index: int| #![auto] position <= index < before.len() implies
            WorkingEntry::clone_equivalent(&before[index], &entries@[index + 1]) by {
            WorkingEntry::clone_reflexive(&before[index]);
        }
        assert forall |index: int| #![auto] 0 <= index < position implies
            before[index].spec_id().spec_order(&entry.spec_id())
                != core::cmp::Ordering::Greater by {}
        assert(exists |inserted: int| {
            &&& inserted == position as int
            &&& 0 <= inserted <= before.len()
            &&& entries@.len() == before.len() + 1
            &&& WorkingEntry::clone_equivalent(entry, &entries@[inserted])
            &&& forall |index: int| #![auto] 0 <= index < inserted ==>
                WorkingEntry::clone_equivalent(&before[index], &entries@[index])
            &&& forall |index: int| #![auto] inserted <= index < before.len() ==>
                WorkingEntry::clone_equivalent(&before[index], &entries@[index + 1])
            &&& forall |index: int| #![auto] 0 <= index < inserted ==>
                before[index].spec_id().spec_order(&entry.spec_id())
                    != core::cmp::Ordering::Greater
            &&& (inserted == before.len()
                || before[inserted].spec_id().spec_order(&entry.spec_id())
                    == core::cmp::Ordering::Greater)
        });
    };
}
}
