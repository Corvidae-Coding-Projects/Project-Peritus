//! Sequence lemmas for the canonical resource-vector merge.

use vstd::prelude::*;

#[cfg(verus_only)]
use crate::{ResourceEntry, ResourceKind};

#[cfg(verus_only)]
use super::{added_entries, entries_ordered};

verus! {

pub(super) proof fn ordered_take(entries: Seq<ResourceEntry>, end: int)
    requires
        crate::resource::capacity::entries_canonical(entries),
        0 <= end <= entries.len(),
    ensures entries_ordered(entries.take(end)),
{
    assert forall |index: int| #![trigger entries.take(end)[index]]
        1 <= index < entries.take(end).len() implies
        entries.take(end)[index - 1].spec_kind().spec_tag()
            < entries.take(end)[index].spec_kind().spec_tag() by {
        assert(entries.take(end)[index - 1] == entries[index - 1]);
        assert(entries.take(end)[index] == entries[index]);
    }
}

pub(super) proof fn prefix_before(entries: Seq<ResourceEntry>, end: int)
    requires
        crate::resource::capacity::entries_canonical(entries),
        0 <= end < entries.len(),
    ensures
        forall |index: int| #![trigger entries.take(end)[index]]
            0 <= index < entries.take(end).len() ==>
            entries.take(end)[index].spec_kind().spec_tag()
                < entries[end].spec_kind().spec_tag(),
{
    assert forall |index: int| #![trigger entries.take(end)[index]]
        0 <= index < entries.take(end).len() implies
        entries.take(end)[index].spec_kind().spec_tag()
            < entries[end].spec_kind().spec_tag() by {
        assert(entries.take(end)[index] == entries[index]);
        crate::resource::capacity::earlier_tag_is_less(entries, index, end);
    }
}

proof fn entries_quantity_bounded(entries: Seq<ResourceEntry>, kind: ResourceKind)
    ensures
        0 <= crate::resource::capacity::entries_quantity(entries, kind)
            <= u64::MAX as int,
    decreases entries.len(),
{
    reveal(crate::resource::capacity::entries_quantity);
    if entries.len() > 0 && entries.first().spec_kind() != kind {
        entries_quantity_bounded(entries.drop_first(), kind);
    }
}

pub(super) proof fn quantities_exclude_overflow(
    result: Seq<ResourceEntry>,
    left: Seq<ResourceEntry>,
    right: Seq<ResourceEntry>,
)
    requires
        crate::resource::capacity::entries_canonical(left),
        crate::resource::capacity::entries_canonical(right),
        forall |kind: ResourceKind| #![auto]
            crate::resource::capacity::entries_quantity(result, kind)
                == crate::resource::capacity::entries_quantity(left, kind)
                    + crate::resource::capacity::entries_quantity(right, kind),
    ensures !super::addition_overflows(left, right),
{
    if super::addition_overflows(left, right) {
        reveal(super::addition_overflows);
        let pair = choose |pair: (int, int)| #![trigger left[pair.0], right[pair.1]]
            0 <= pair.0 < left.len()
                && 0 <= pair.1 < right.len()
                && left[pair.0].spec_kind() == right[pair.1].spec_kind()
                && left[pair.0].spec_quantity().spec_value() as int
                    + right[pair.1].spec_quantity().spec_value() as int
                    > u64::MAX as int;
        let left_index = pair.0;
        let right_index = pair.1;
        let kind = left[left_index].spec_kind();
        crate::resource::capacity::exact_quantity_at(left, left_index, kind);
        crate::resource::capacity::exact_quantity_at(right, right_index, kind);
        entries_quantity_bounded(result, kind);
    }
}

pub(super) proof fn equal_quantities_advance(
    result: Seq<ResourceEntry>,
    left: Seq<ResourceEntry>,
    right: Seq<ResourceEntry>,
    left_index: int,
    right_index: int,
    next: ResourceEntry,
)
    requires
        crate::resource::capacity::entries_canonical(left),
        crate::resource::capacity::entries_canonical(right),
        0 <= left_index < left.len(),
        0 <= right_index < right.len(),
        left[left_index].spec_kind() == right[right_index].spec_kind(),
        next.spec_kind() == left[left_index].spec_kind(),
        next.spec_quantity().spec_value() as int
            == left[left_index].spec_quantity().spec_value() as int
                + right[right_index].spec_quantity().spec_value() as int,
        entries_ordered(result),
        forall |index: int| #![trigger result[index]] 0 <= index < result.len() ==>
            result[index].spec_kind().spec_tag()
                < next.spec_kind().spec_tag(),
        forall |kind: ResourceKind| #![auto]
            crate::resource::capacity::entries_quantity(result, kind)
                == crate::resource::capacity::entries_quantity(
                    left.take(left_index), kind)
                    + crate::resource::capacity::entries_quantity(
                        right.take(right_index), kind),
    ensures
        forall |kind: ResourceKind| #![auto]
            crate::resource::capacity::entries_quantity(result.push(next), kind)
                == crate::resource::capacity::entries_quantity(
                    left.take(left_index + 1), kind)
                    + crate::resource::capacity::entries_quantity(
                        right.take(right_index + 1), kind),
{
    let left_prefix = left.take(left_index);
    let right_prefix = right.take(right_index);
    ordered_take(left, left_index);
    prefix_before(left, left_index);
    appended_quantities(left_prefix, left[left_index]);
    ordered_take(right, right_index);
    prefix_before(right, right_index);
    appended_quantities(right_prefix, right[right_index]);
    appended_quantities(result, next);
    assert(left.take(left_index + 1) == left_prefix.push(left[left_index]));
    assert(right.take(right_index + 1) == right_prefix.push(right[right_index]));
}

proof fn append_quantity(
    entries: Seq<ResourceEntry>,
    entry: ResourceEntry,
    kind: ResourceKind,
)
    requires
        entries_ordered(entries),
        forall |index: int| #![trigger entries[index]] 0 <= index < entries.len() ==>
            entries[index].spec_kind().spec_tag() < entry.spec_kind().spec_tag(),
    ensures
        crate::resource::capacity::entries_quantity(entries.push(entry), kind)
            == crate::resource::capacity::entries_quantity(entries, kind)
                + if entry.spec_kind() == kind {
                    entry.spec_quantity().spec_value() as int
                } else {
                    0int
                },
    decreases entries.len(),
{
    reveal(crate::resource::capacity::entries_quantity);
    if entries.len() == 0 {
        assert(entries.push(entry).len() == 1);
        assert(entries.push(entry).first() == entry);
        assert(entries.push(entry).drop_first().len() == 0);
        assert(entries.push(entry).drop_first() =~= entries);
        assert(entries.push(entry).drop_first() == entries);
        reveal_with_fuel(crate::resource::capacity::entries_quantity, 2);
        if entry.spec_kind() == kind {
            assert(crate::resource::capacity::entries_quantity(
                entries.push(entry), kind)
                    == entry.spec_quantity().spec_value() as int);
        } else {
            assert(crate::resource::capacity::entries_quantity(
                entries.push(entry), kind) == 0);
        }
        assert(crate::resource::capacity::entries_quantity(entries, kind) == 0);
    } else {
        assert(entries.push(entry).first() == entries.first());
        assert(entries.push(entry).drop_first() =~= entries.drop_first().push(entry));
        assert(entries.first().spec_kind() != entry.spec_kind());
        assert forall |index: int| #![trigger entries.drop_first()[index]]
            0 <= index < entries.drop_first().len() implies
            entries.drop_first()[index].spec_kind().spec_tag()
                < entry.spec_kind().spec_tag() by {
            assert(entries.drop_first()[index] == entries[index + 1]);
        }
        assert(entries_ordered(entries.drop_first())) by {
            assert forall |index: int| #![trigger entries.drop_first()[index]]
                1 <= index < entries.drop_first().len() implies
                entries.drop_first()[index - 1].spec_kind().spec_tag()
                    < entries.drop_first()[index].spec_kind().spec_tag() by {
                assert(entries.drop_first()[index - 1] == entries[index]);
                assert(entries.drop_first()[index] == entries[index + 1]);
            }
        }
        if entries.first().spec_kind() != kind {
            append_quantity(entries.drop_first(), entry, kind);
            reveal(crate::resource::capacity::entries_quantity);
            assert(crate::resource::capacity::entries_quantity(
                entries.push(entry), kind)
                    == crate::resource::capacity::entries_quantity(
                        entries.drop_first().push(entry), kind));
            assert(crate::resource::capacity::entries_quantity(entries, kind)
                == crate::resource::capacity::entries_quantity(entries.drop_first(), kind));
        } else {
            assert(entry.spec_kind() != kind);
            reveal(crate::resource::capacity::entries_quantity);
            assert(crate::resource::capacity::entries_quantity(
                entries.push(entry), kind)
                    == entries.first().spec_quantity().spec_value() as int);
            assert(crate::resource::capacity::entries_quantity(entries, kind)
                == entries.first().spec_quantity().spec_value() as int);
        }
    }
}

pub(super) proof fn ordered_push(entries: Seq<ResourceEntry>, entry: ResourceEntry)
    requires
        entries_ordered(entries),
        forall |index: int| #![trigger entries[index]] 0 <= index < entries.len() ==>
            entries[index].spec_kind().spec_tag() < entry.spec_kind().spec_tag(),
    ensures entries_ordered(entries.push(entry)),
{
    assert forall |index: int| #![trigger entries.push(entry)[index]]
        1 <= index < entries.push(entry).len() implies
        entries.push(entry)[index - 1].spec_kind().spec_tag()
            < entries.push(entry)[index].spec_kind().spec_tag() by {
        if index < entries.len() {
            assert(entries.push(entry)[index - 1] == entries[index - 1]);
            assert(entries.push(entry)[index] == entries[index]);
        } else {
            assert(index == entries.len());
        }
    }
}

pub(super) proof fn appended_quantities(entries: Seq<ResourceEntry>, entry: ResourceEntry)
    requires
        entries_ordered(entries),
        forall |index: int| #![trigger entries[index]] 0 <= index < entries.len() ==>
            entries[index].spec_kind().spec_tag() < entry.spec_kind().spec_tag(),
    ensures
        forall |kind: ResourceKind| #![auto]
            crate::resource::capacity::entries_quantity(entries.push(entry), kind)
                == crate::resource::capacity::entries_quantity(entries, kind)
                    + if entry.spec_kind() == kind {
                        entry.spec_quantity().spec_value() as int
                    } else {
                        0int
                    },
{
    assert forall |kind: ResourceKind| #![auto]
        crate::resource::capacity::entries_quantity(entries.push(entry), kind)
            == crate::resource::capacity::entries_quantity(entries, kind)
                + if entry.spec_kind() == kind {
                    entry.spec_quantity().spec_value() as int
                } else {
                    0int
                } by {
        append_quantity(entries, entry, kind);
    }
}

pub(super) proof fn added_entries_push_left(
    left: Seq<ResourceEntry>,
    right: Seq<ResourceEntry>,
    entry: ResourceEntry,
)
    requires
        left.len() == 0 || left.last().spec_kind().spec_tag() < entry.spec_kind().spec_tag(),
        right.len() == 0 || right.last().spec_kind().spec_tag() < entry.spec_kind().spec_tag(),
    ensures
        added_entries(left.push(entry), right) == added_entries(left, right).push(entry),
{
    assert(left.push(entry).last() == entry);
    assert(left.push(entry).drop_last() =~= left);
    if right.len() > 0 {
        assert(entry.spec_kind().spec_tag() > right.last().spec_kind().spec_tag());
    }
    reveal(added_entries);
}

pub(super) proof fn added_entries_push_right(
    left: Seq<ResourceEntry>,
    right: Seq<ResourceEntry>,
    entry: ResourceEntry,
)
    requires
        left.len() == 0 || left.last().spec_kind().spec_tag() < entry.spec_kind().spec_tag(),
        right.len() == 0 || right.last().spec_kind().spec_tag() < entry.spec_kind().spec_tag(),
    ensures
        added_entries(left, right.push(entry)) == added_entries(left, right).push(entry),
{
    assert(right.push(entry).last() == entry);
    assert(right.push(entry).drop_last() =~= right);
    if left.len() > 0 {
        assert(entry.spec_kind().spec_tag() > left.last().spec_kind().spec_tag());
    }
    reveal(added_entries);
}

} // verus!
