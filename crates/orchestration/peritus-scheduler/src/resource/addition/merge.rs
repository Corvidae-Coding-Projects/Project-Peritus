//! Executable merge state for exact canonical resource addition.

use vstd::prelude::*;

use std::cmp::Ordering;

use crate::{ResourceEntry, ResourceVector};

#[cfg(verus_only)]
use crate::ResourceKind;

mod advance_proofs;

verus! {

pub(super) enum MergeOutcome {
    Value(Vec<ResourceEntry>),
    Overflow,
}

struct MergeCursor {
    entries: Vec<ResourceEntry>,
    left: usize,
    right: usize,
}

enum MergeAdvance {
    Value(MergeCursor),
    Overflow,
}

closed spec fn cursor_valid(
    cursor: &MergeCursor,
    left_entries: Seq<ResourceEntry>,
    right_entries: Seq<ResourceEntry>,
) -> bool {
    &&& cursor.left <= left_entries.len()
    &&& cursor.right <= right_entries.len()
    &&& crate::resource::capacity::entries_canonical(left_entries)
    &&& crate::resource::capacity::entries_canonical(right_entries)
    &&& super::entries_ordered(cursor.entries@)
    &&& cursor.entries@.len() <= cursor.left + cursor.right
    &&& cursor.entries@ == super::added_entries(
        left_entries.take(cursor.left as int), right_entries.take(cursor.right as int))
    &&& forall |kind: ResourceKind| #![auto]
        crate::resource::capacity::entries_quantity(cursor.entries@, kind)
            == crate::resource::capacity::entries_quantity(
                left_entries.take(cursor.left as int), kind)
                + crate::resource::capacity::entries_quantity(
                    right_entries.take(cursor.right as int), kind)
    &&& cursor.left < left_entries.len() ==> forall |index: int|
        #![trigger cursor.entries@[index]]
        0 <= index < cursor.entries@.len() ==>
        cursor.entries@[index].spec_kind().spec_tag()
            < left_entries[cursor.left as int].spec_kind().spec_tag()
    &&& cursor.right < right_entries.len() ==> forall |index: int|
        #![trigger cursor.entries@[index]]
        0 <= index < cursor.entries@.len() ==>
        cursor.entries@[index].spec_kind().spec_tag()
            < right_entries[cursor.right as int].spec_kind().spec_tag()
    &&& cursor.left < left_entries.len() && cursor.right > 0 ==>
        right_entries[cursor.right as int - 1].spec_kind().spec_tag()
            < left_entries[cursor.left as int].spec_kind().spec_tag()
    &&& cursor.right < right_entries.len() && cursor.left > 0 ==>
        left_entries[cursor.left as int - 1].spec_kind().spec_tag()
            < right_entries[cursor.right as int].spec_kind().spec_tag()
}

fn advance_equal(
    mut cursor: MergeCursor,
    left_entries: &[ResourceEntry],
    right_entries: &[ResourceEntry],
) -> (outcome: MergeAdvance)
    requires
        cursor_valid(&cursor, left_entries@, right_entries@),
        cursor.left < left_entries@.len(),
        cursor.right < right_entries@.len(),
        left_entries@[cursor.left as int].spec_kind()
            == right_entries@[cursor.right as int].spec_kind(),
    ensures match outcome {
        MergeAdvance::Value(next) => {
            &&& cursor_valid(&next, left_entries@, right_entries@)
            &&& left_entries@.len() + right_entries@.len() - next.left - next.right
                < left_entries@.len() + right_entries@.len() - cursor.left - cursor.right
        }
        MergeAdvance::Overflow => super::addition_overflows(left_entries@, right_entries@),
    },
{
    reveal(cursor_valid);
    let ghost left_prefix = left_entries@.take(cursor.left as int);
    let ghost right_prefix = right_entries@.take(cursor.right as int);
    let left = &left_entries[cursor.left];
    let right = &right_entries[cursor.right];
    let Some(next) = super::combined_entry(left, right) else {
        assert(super::addition_overflows(left_entries@, right_entries@)) by {
            let left_index = cursor.left as int;
            let right_index = cursor.right as int;
            assert(left_entries@[left_index] == *left);
            assert(right_entries@[right_index] == *right);
            reveal(super::addition_overflows);
            let pair = (left_index, right_index);
            assert(0 <= pair.0 < left_entries@.len());
            assert(0 <= pair.1 < right_entries@.len());
            assert(exists |witness: (int, int)|
                #![trigger left_entries@[witness.0], right_entries@[witness.1]]
                0 <= witness.0 < left_entries@.len()
                    && 0 <= witness.1 < right_entries@.len()
                    && left_entries@[witness.0].spec_kind()
                        == right_entries@[witness.1].spec_kind()
                    && left_entries@[witness.0].spec_quantity().spec_value() as int
                        + right_entries@[witness.1].spec_quantity().spec_value() as int
                        > u64::MAX as int) by {
                assert(left_entries@[pair.0].spec_kind()
                    == right_entries@[pair.1].spec_kind());
            }
        }
        return MergeAdvance::Overflow;
    };
    proof {
        super::proofs::equal_quantities_advance(
            cursor.entries@,
            left_entries@,
            right_entries@,
            cursor.left as int,
            cursor.right as int,
            next,
        );
        super::added_entries_push_both(left_prefix, right_prefix, *left, *right);
        assert(left_entries@.take(cursor.left as int + 1) == left_prefix.push(*left));
        assert(right_entries@.take(cursor.right as int + 1) == right_prefix.push(*right));
        super::proofs::ordered_push(cursor.entries@, next);
        super::proofs::appended_quantities(cursor.entries@, next);
    }
    cursor.left += 1;
    cursor.right += 1;
    cursor.entries.push(next);
    MergeAdvance::Value(cursor)
}

fn advance_left(
    mut cursor: MergeCursor,
    left_entries: &[ResourceEntry],
    _right_entries: &[ResourceEntry],
) -> (next: MergeCursor)
    requires
        cursor_valid(&cursor, left_entries@, _right_entries@),
        cursor.left < left_entries@.len(),
        cursor.right >= _right_entries@.len()
            || left_entries@[cursor.left as int].spec_kind().spec_tag()
                < _right_entries@[cursor.right as int].spec_kind().spec_tag(),
    ensures
        cursor_valid(&next, left_entries@, _right_entries@),
        left_entries@.len() + _right_entries@.len() - next.left - next.right
            < left_entries@.len() + _right_entries@.len() - cursor.left - cursor.right,
{
    reveal(cursor_valid);
    let ghost left_prefix = left_entries@.take(cursor.left as int);
    let ghost right_prefix = _right_entries@.take(cursor.right as int);
    let entry = left_entries[cursor.left];
    proof {
        super::proofs::ordered_take(left_entries@, cursor.left as int);
        super::proofs::prefix_before(left_entries@, cursor.left as int);
        super::proofs::appended_quantities(left_prefix, entry);
        super::proofs::added_entries_push_left(left_prefix, right_prefix, entry);
        assert(left_entries@.take(cursor.left as int + 1) == left_prefix.push(entry));
        super::proofs::ordered_push(cursor.entries@, entry);
        super::proofs::appended_quantities(cursor.entries@, entry);
    }
    cursor.left += 1;
    cursor.entries.push(entry);
    cursor
}

fn advance_right(
    mut cursor: MergeCursor,
    _left_entries: &[ResourceEntry],
    right_entries: &[ResourceEntry],
) -> (next: MergeCursor)
    requires
        cursor_valid(&cursor, _left_entries@, right_entries@),
        cursor.right < right_entries@.len(),
        cursor.left >= _left_entries@.len()
            || right_entries@[cursor.right as int].spec_kind().spec_tag()
                < _left_entries@[cursor.left as int].spec_kind().spec_tag(),
    ensures
        cursor_valid(&next, _left_entries@, right_entries@),
        _left_entries@.len() + right_entries@.len() - next.left - next.right
            < _left_entries@.len() + right_entries@.len() - cursor.left - cursor.right,
{
    reveal(cursor_valid);
    let ghost before_entries = cursor.entries@;
    let ghost before_left = cursor.left;
    let ghost before_right = cursor.right;
    let entry = right_entries[cursor.right];
    proof {
        advance_proofs::advance_right_parts(
            before_entries,
            before_left as nat,
            before_right as nat,
            _left_entries@,
            right_entries@,
        );
    }
    cursor.right += 1;
    cursor.entries.push(entry);
    proof {
        assert(cursor.entries@ == before_entries.push(entry));
        assert(cursor.left == before_left);
        assert(cursor.right == before_right + 1);
        assert(cursor_valid(&cursor, _left_entries@, right_entries@));
    }
    cursor
}

fn advance(
    cursor: MergeCursor,
    left_entries: &[ResourceEntry],
    right_entries: &[ResourceEntry],
) -> (outcome: MergeAdvance)
    requires
        cursor_valid(&cursor, left_entries@, right_entries@),
        cursor.left < left_entries@.len() || cursor.right < right_entries@.len(),
    ensures match outcome {
        MergeAdvance::Value(next) => {
            &&& cursor_valid(&next, left_entries@, right_entries@)
            &&& left_entries@.len() + right_entries@.len() - next.left - next.right
                < left_entries@.len() + right_entries@.len() - cursor.left - cursor.right
        }
        MergeAdvance::Overflow => super::addition_overflows(left_entries@, right_entries@),
    },
{
    if cursor.left >= left_entries.len() {
        MergeAdvance::Value(advance_right(cursor, left_entries, right_entries))
    } else if cursor.right >= right_entries.len() {
        MergeAdvance::Value(advance_left(cursor, left_entries, right_entries))
    } else {
        let left_tag = left_entries[cursor.left].kind().tag();
        let right_tag = right_entries[cursor.right].kind().tag();
        match left_tag.cmp(&right_tag) {
            Ordering::Equal => {
                assert(left_tag == right_tag);
                advance_equal(cursor, left_entries, right_entries)
            }
            Ordering::Less => {
                assert(left_tag < right_tag);
                MergeAdvance::Value(advance_left(cursor, left_entries, right_entries))
            }
            Ordering::Greater => {
                assert(left_tag > right_tag);
                MergeAdvance::Value(advance_right(cursor, left_entries, right_entries))
            }
        }
    }
}

pub(super) fn merge_entries(
    left_vector: &ResourceVector,
    right_vector: &ResourceVector,
) -> (outcome: MergeOutcome)
    ensures match outcome {
        MergeOutcome::Value(entries) => {
            &&& !super::addition_overflows(
                left_vector.spec_entries(), right_vector.spec_entries())
            &&& entries@ == super::added_entries(
                left_vector.spec_entries(), right_vector.spec_entries())
            &&& forall |kind: ResourceKind| #![auto]
                crate::resource::capacity::entries_quantity(entries@, kind)
                    == left_vector.spec_quantity(kind) + right_vector.spec_quantity(kind)
            &&& entries@.len() == super::union_dimensions(
                left_vector.spec_entries(), right_vector.spec_entries())
            &&& entries@.len() > 0
            &&& super::entries_ordered(entries@)
        }
        MergeOutcome::Overflow => super::addition_overflows(
            left_vector.spec_entries(), right_vector.spec_entries()),
    },
{
    proof {
        use_type_invariant(left_vector);
        use_type_invariant(right_vector);
    }
    let left_entries = left_vector.entries();
    let right_entries = right_vector.entries();
    let mut cursor = MergeCursor {
        entries: Vec::with_capacity(left_entries.len().saturating_add(right_entries.len())),
        left: 0,
        right: 0,
    };
    proof {
        reveal(cursor_valid);
        reveal(super::added_entries);
        assert(cursor.entries@ =~= Seq::<ResourceEntry>::empty());
        assert(left_entries@.take(0).len() == 0);
        assert(right_entries@.take(0).len() == 0);
        assert(left_entries@.take(0) =~= Seq::<ResourceEntry>::empty());
        assert(right_entries@.take(0) =~= Seq::<ResourceEntry>::empty());
        assert(super::entries_ordered(cursor.entries@)) by {
            assert forall |index: int| #![trigger cursor.entries@[index]]
                1 <= index < cursor.entries@.len() implies
                cursor.entries@[index - 1].spec_kind().spec_tag()
                    < cursor.entries@[index].spec_kind().spec_tag() by {
                assert(false);
            }
        }
        assert forall |kind: ResourceKind| #![auto]
            crate::resource::capacity::entries_quantity(cursor.entries@, kind)
                == crate::resource::capacity::entries_quantity(
                    left_entries@.take(0), kind)
                    + crate::resource::capacity::entries_quantity(
                        right_entries@.take(0), kind) by {
            reveal(crate::resource::capacity::entries_quantity);
        }
        if left_entries@.len() > 0 {
            assert forall |index: int| #![trigger cursor.entries@[index]]
                0 <= index < cursor.entries@.len() implies
                cursor.entries@[index].spec_kind().spec_tag()
                    < left_entries@[0].spec_kind().spec_tag() by {
                assert(false);
            }
        }
        if right_entries@.len() > 0 {
            assert forall |index: int| #![trigger cursor.entries@[index]]
                0 <= index < cursor.entries@.len() implies
                cursor.entries@[index].spec_kind().spec_tag()
                    < right_entries@[0].spec_kind().spec_tag() by {
                assert(false);
            }
        }
        assert(cursor_valid(&cursor, left_entries@, right_entries@));
    }
    while cursor.left < left_entries.len() || cursor.right < right_entries.len()
        invariant
            left_entries@ == left_vector.spec_entries(),
            right_entries@ == right_vector.spec_entries(),
            cursor_valid(&cursor, left_entries@, right_entries@),
        decreases left_entries@.len() + right_entries@.len() - cursor.left - cursor.right,
    {
        cursor = match advance(cursor, left_entries, right_entries) {
            MergeAdvance::Value(next) => next,
            MergeAdvance::Overflow => return MergeOutcome::Overflow,
        };
    }
    reveal(cursor_valid);
    assert(cursor.left == left_entries@.len());
    assert(cursor.right == right_entries@.len());
    assert(left_entries@.take(cursor.left as int) =~= left_entries@);
    assert(right_entries@.take(cursor.right as int) =~= right_entries@);
    assert(cursor.entries@ == super::added_entries(left_entries@, right_entries@));
    assert(cursor.entries@.len() == super::union_dimensions(left_entries@, right_entries@));
    assert(cursor.entries@.len() > 0);
    assert(super::entries_ordered(cursor.entries@));
    assert forall |kind: ResourceKind| #![auto]
        crate::resource::capacity::entries_quantity(cursor.entries@, kind)
            == left_vector.spec_quantity(kind) + right_vector.spec_quantity(kind) by {
        left_vector.quantity_matches_entries(kind);
        right_vector.quantity_matches_entries(kind);
    }
    proof {
        super::proofs::quantities_exclude_overflow(
            cursor.entries@, left_entries@, right_entries@);
    }
    assert(!super::addition_overflows(
        left_vector.spec_entries(), right_vector.spec_entries()));
    MergeOutcome::Value(cursor.entries)
}

} // verus!
