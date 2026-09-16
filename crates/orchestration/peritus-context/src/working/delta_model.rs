//! Logical exact-result model for ordered working-state deltas.

#[cfg(verus_only)]
use super::validation::spec_find_entry_from;
#[cfg(verus_only)]
use super::{WorkingEntry, WorkingEntryStatus};
use vstd::prelude::*;

verus! {
pub(super) open spec fn spec_one_upsert(
    before: Seq<WorkingEntry>,
    proposed: &WorkingEntry,
    after: Seq<WorkingEntry>,
) -> bool {
    match spec_find_entry_from(before, proposed.spec_id(), 0) {
        Some(position) => {
            &&& after.len() == before.len()
            &&& WorkingEntry::clone_equivalent(proposed, &after[position])
            &&& forall |index: int| #![auto] 0 <= index < before.len() && index != position ==>
                WorkingEntry::clone_equivalent(&before[index], &after[index])
        }
        None => exists |position: int| {
            &&& 0 <= position <= before.len()
            &&& after.len() == before.len() + 1
            &&& WorkingEntry::clone_equivalent(proposed, &after[position])
            &&& forall |index: int| #![auto] 0 <= index < position ==>
                WorkingEntry::clone_equivalent(&before[index], &after[index])
            &&& forall |index: int| #![auto] position <= index < before.len() ==>
                WorkingEntry::clone_equivalent(&before[index], &after[index + 1])
            &&& forall |index: int| #![auto] 0 <= index < position ==>
                before[index].spec_id().spec_order(&proposed.spec_id())
                    != core::cmp::Ordering::Greater
            &&& (position == before.len()
                || before[position].spec_id().spec_order(&proposed.spec_id())
                    == core::cmp::Ordering::Greater)
        },
    }
}

pub(super) open spec fn spec_upsert_prefix(
    initial: Seq<WorkingEntry>,
    proposals: Seq<WorkingEntry>,
    count: int,
    current: Seq<WorkingEntry>,
) -> bool
    decreases count,
{
    if count <= 0 {
        WorkingEntry::sequence_clone_equivalent(initial, current)
    } else {
        count <= proposals.len()
            && exists |previous: Seq<WorkingEntry>|
                spec_upsert_prefix(initial, proposals, count - 1, previous)
                    && #[trigger] spec_one_upsert(previous, &proposals[count - 1], current)
    }
}

pub(super) open spec fn spec_one_supersession(
    before: Seq<WorkingEntry>,
    proposed: &WorkingEntry,
    after: Seq<WorkingEntry>,
) -> bool {
    match proposed.spec_supersedes() {
        None => WorkingEntry::sequence_clone_equivalent(before, after),
        Some(previous) => match spec_find_entry_from(before, previous, 0) {
            None => false,
            Some(position) => {
                &&& after.len() == before.len()
                &&& WorkingEntry::payload_equivalent(&before[position], &after[position])
                &&& after[position].spec_status() == WorkingEntryStatus::Superseded
                &&& after[position].spec_stale_through()
                    == before[position].spec_stale_through()
                &&& forall |index: int| #![auto] 0 <= index < before.len() && index != position ==>
                    WorkingEntry::clone_equivalent(&before[index], &after[index])
            }
        },
    }
}

pub(super) open spec fn spec_supersession_prefix(
    initial: Seq<WorkingEntry>,
    proposals: Seq<WorkingEntry>,
    count: int,
    current: Seq<WorkingEntry>,
) -> bool
    decreases count,
{
    if count <= 0 {
        WorkingEntry::sequence_clone_equivalent(initial, current)
    } else {
        count <= proposals.len()
            && exists |previous: Seq<WorkingEntry>|
                spec_supersession_prefix(initial, proposals, count - 1, previous)
                    && #[trigger] spec_one_supersession(previous, &proposals[count - 1], current)
    }
}

}
