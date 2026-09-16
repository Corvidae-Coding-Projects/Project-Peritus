//! Solver-bounded proof step for appending one right-hand merge entry.

use vstd::prelude::*;

#[cfg(verus_only)]
use crate::{ResourceEntry, ResourceKind};

#[cfg(verus_only)]
use super::super::{added_entries, entries_ordered};

verus! {

pub(super) proof fn advance_right_parts(
    entries: Seq<ResourceEntry>,
    left: nat,
    right: nat,
    left_entries: Seq<ResourceEntry>,
    right_entries: Seq<ResourceEntry>,
)
    requires
        left <= left_entries.len(),
        right < right_entries.len(),
        crate::resource::capacity::entries_canonical(left_entries),
        crate::resource::capacity::entries_canonical(right_entries),
        entries_ordered(entries),
        entries.len() <= left + right,
        entries == added_entries(
            left_entries.take(left as int), right_entries.take(right as int)),
        forall |kind: ResourceKind| #![auto]
            crate::resource::capacity::entries_quantity(entries, kind)
                == crate::resource::capacity::entries_quantity(
                    left_entries.take(left as int), kind)
                    + crate::resource::capacity::entries_quantity(
                        right_entries.take(right as int), kind),
        left < left_entries.len() ==> forall |index: int| #![trigger entries[index]]
            0 <= index < entries.len() ==>
                entries[index].spec_kind().spec_tag()
                    < left_entries[left as int].spec_kind().spec_tag(),
        forall |index: int| #![trigger entries[index]]
            0 <= index < entries.len() ==>
                entries[index].spec_kind().spec_tag()
                    < right_entries[right as int].spec_kind().spec_tag(),
        left < left_entries.len() && right > 0 ==>
            right_entries[right as int - 1].spec_kind().spec_tag()
                < left_entries[left as int].spec_kind().spec_tag(),
        left > 0 ==> left_entries[left as int - 1].spec_kind().spec_tag()
            < right_entries[right as int].spec_kind().spec_tag(),
        left >= left_entries.len()
            || right_entries[right as int].spec_kind().spec_tag()
                < left_entries[left as int].spec_kind().spec_tag(),
    ensures
        left <= left_entries.len(),
        right + 1 <= right_entries.len(),
        crate::resource::capacity::entries_canonical(left_entries),
        crate::resource::capacity::entries_canonical(right_entries),
        entries_ordered(entries.push(right_entries[right as int])),
        entries.push(right_entries[right as int]).len() <= left + right + 1,
        entries.push(right_entries[right as int]) == added_entries(
            left_entries.take(left as int), right_entries.take(right as int + 1)),
        forall |kind: ResourceKind| #![auto]
            crate::resource::capacity::entries_quantity(
                entries.push(right_entries[right as int]), kind)
                == crate::resource::capacity::entries_quantity(
                    left_entries.take(left as int), kind)
                    + crate::resource::capacity::entries_quantity(
                        right_entries.take(right as int + 1), kind),
        left < left_entries.len() ==> forall |index: int|
            #![trigger entries.push(right_entries[right as int])[index]]
            0 <= index < entries.push(right_entries[right as int]).len() ==>
                entries.push(right_entries[right as int])[index].spec_kind().spec_tag()
                    < left_entries[left as int].spec_kind().spec_tag(),
        right + 1 < right_entries.len() ==> forall |index: int|
            #![trigger entries.push(right_entries[right as int])[index]]
            0 <= index < entries.push(right_entries[right as int]).len() ==>
                entries.push(right_entries[right as int])[index].spec_kind().spec_tag()
                    < right_entries[(right + 1) as int].spec_kind().spec_tag(),
        left < left_entries.len() ==> right_entries[right as int].spec_kind().spec_tag()
            < left_entries[left as int].spec_kind().spec_tag(),
        right + 1 < right_entries.len() && left > 0 ==>
            left_entries[left as int - 1].spec_kind().spec_tag()
                < right_entries[(right + 1) as int].spec_kind().spec_tag(),
{
    let left_prefix = left_entries.take(left as int);
    let right_prefix = right_entries.take(right as int);
    let entry = right_entries[right as int];

    super::super::proofs::ordered_take(right_entries, right as int);
    super::super::proofs::prefix_before(right_entries, right as int);
    super::super::proofs::appended_quantities(right_prefix, entry);
    super::super::proofs::added_entries_push_right(left_prefix, right_prefix, entry);
    assert(right_entries.take(right as int + 1) == right_prefix.push(entry));
    super::super::proofs::ordered_push(entries, entry);
    super::super::proofs::appended_quantities(entries, entry);
    assert(entries.push(entry).len() == entries.len() + 1);
    assert(entries.push(entry) == added_entries(
        left_entries.take(left as int), right_entries.take(right as int + 1)));

    assert forall |kind: ResourceKind| #![auto]
        crate::resource::capacity::entries_quantity(entries.push(entry), kind)
            == crate::resource::capacity::entries_quantity(left_prefix, kind)
                + crate::resource::capacity::entries_quantity(
                    right_entries.take(right as int + 1), kind) by {
    }
    if left < left_entries.len() {
        assert(entry.spec_kind().spec_tag()
            < left_entries[left as int].spec_kind().spec_tag());
        assert forall |index: int|
            #![trigger entries.push(entry)[index]]
            0 <= index < entries.push(entry).len() implies
                entries.push(entry)[index].spec_kind().spec_tag()
                    < left_entries[left as int].spec_kind().spec_tag() by {
            if index < entries.len() {
                assert(entries.push(entry)[index] == entries[index]);
            } else {
                assert(index == entries.len());
                assert(entries.push(entry)[index] == entry);
            }
        }
    }
    if right + 1 < right_entries.len() {
        crate::resource::capacity::earlier_tag_is_less(
            right_entries,
            right as int,
            right as int + 1,
        );
        assert forall |index: int|
            #![trigger entries.push(entry)[index]]
            0 <= index < entries.push(entry).len() implies
                entries.push(entry)[index].spec_kind().spec_tag()
                    < right_entries[(right + 1) as int].spec_kind().spec_tag() by {
            if index < entries.len() {
                assert(entries.push(entry)[index] == entries[index]);
            } else {
                assert(index == entries.len());
                assert(entries.push(entry)[index] == entry);
            }
        }
    }
}

} // verus!
