//! Verified canonical resource subtraction without output revalidation.

use vstd::prelude::*;

use super::{ResourceEntry, ResourceQuantity, ResourceVector, VectorSubtraction};

verus! {

/// A left-hand dimension would underflow before sentinel validation.
pub(super) open spec fn subtraction_underflows(
    left: &ResourceVector,
    other: &ResourceVector,
) -> bool {
    exists |index: int| #![trigger left.spec_entries()[index]]
        0 <= index < left.spec_entries().len()
            && left.spec_entries()[index].spec_quantity().spec_value()
                < other.spec_quantity(left.spec_entries()[index].spec_kind())
}

/// The subtrahend names a dimension whose left lookup is the zero sentinel.
pub(super) open spec fn subtraction_has_zero_sentinel(
    left: &ResourceVector,
    other: &ResourceVector,
) -> bool {
    exists |index: int| #![trigger other.spec_entries()[index]]
        0 <= index < other.spec_entries().len()
            && left.spec_quantity(other.spec_entries()[index].spec_kind()) == 0
}

/// Exact nonzero canonical entries produced after successful subtraction.
pub(super) open spec fn subtracted_entries(
    left: Seq<ResourceEntry>,
    other: &ResourceVector,
) ->Seq<ResourceEntry>
    decreases left.len(),
{
    if left.len() == 0 {
        Seq::empty()
    } else {
        let prior = subtracted_entries(left.drop_last(), other);
        let entry = left.last();
        let remaining = entry.spec_quantity().spec_value() as int
            - other.spec_quantity(entry.spec_kind());
        if remaining <= 0 {
            prior
        } else {
            prior.push(ResourceEntry {
                kind: entry.spec_kind(),
                quantity: ResourceQuantity(remaining as u64),
            })
        }
    }
}

/// Exact successful quantity difference across every resource identity.
pub(super) open spec fn subtraction_result(
    result: &ResourceVector,
    left: &ResourceVector,
    other: &ResourceVector,
) -> bool {
    result.spec_entries() == subtracted_entries(left.spec_entries(), other)
}

/// Returns whether a possibly empty sequence has bounded canonical ordering.
pub(super) open spec fn entries_ordered(entries: Seq<ResourceEntry>) -> bool {
    &&& entries.len() <= u16::MAX as int
    &&& forall |index: int| #![trigger entries[index]] 1 <= index < entries.len() ==>
        entries[index - 1].spec_kind().spec_tag() < entries[index].spec_kind().spec_tag()
}

proof fn ordered_push(entries: Seq<ResourceEntry>, entry: ResourceEntry)
    requires
        entries_ordered(entries),
        entries.len() < u16::MAX as int,
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
            assert(entries.push(entry)[index - 1] == entries[index - 1]);
            assert(entries.push(entry)[index] == entry);
        }
    }
}

proof fn subtracted_entries_push(
    left: Seq<ResourceEntry>,
    other: &ResourceVector,
    entry: ResourceEntry,
)
    ensures
        subtracted_entries(left.push(entry), other) == {
            let remaining = entry.spec_quantity().spec_value() as int
                - other.spec_quantity(entry.spec_kind());
            if remaining <= 0 {
                subtracted_entries(left, other)
            } else {
                subtracted_entries(left, other).push(ResourceEntry {
                    kind: entry.spec_kind(),
                    quantity: ResourceQuantity(remaining as u64),
                })
            }
        },
{
    assert(left.push(entry).last() == entry);
    assert(left.push(entry).drop_last() =~= left);
    reveal(subtracted_entries);
}

enum EntrySubtraction {
    Value(Vec<ResourceEntry>),
    Underflow,
}

fn subtract_entries(
    left: &ResourceVector,
    other: &ResourceVector,
) -> (result: EntrySubtraction)
    ensures match result {
        EntrySubtraction::Underflow => subtraction_underflows(left, other),
        EntrySubtraction::Value(entries) => {
            &&& !subtraction_underflows(left, other)
            &&& entries@ == subtracted_entries(left.spec_entries(), other)
            &&& entries_ordered(entries@)
        }
    },
{
    proof {
        use_type_invariant(left);
        use_type_invariant(other);
    }
    let mut entries = Vec::with_capacity(left.0.len());
    let mut index = 0usize;
    while index < left.0.len()
        invariant
            index <= left.0@.len(),
            crate::resource::capacity::entries_canonical(left.0@),
            crate::resource::capacity::entries_canonical(other.0@),
            entries_ordered(entries@),
            entries@ == subtracted_entries(left.0@.take(index as int), other),
            forall |prior: int| 0 <= prior < index ==>
                left.0@[prior].spec_quantity().spec_value()
                    >= other.spec_quantity(left.0@[prior].spec_kind()),
            entries@.len() <= index,
            index < left.0@.len() ==>
                forall |prior: int| #![trigger entries@[prior]]
                    0 <= prior < entries@.len() ==>
                    entries@[prior].spec_kind().spec_tag()
                        < left.0@[index as int].spec_kind().spec_tag(),
        decreases left.0@.len() - index,
    {
        let entry = left.0[index];
        let ghost prefix = left.0@.take(index as int);
        let subtract = other.quantity(entry.kind());
        if entry.quantity().get() < subtract {
            assert(subtraction_underflows(left, other)) by {
                let witness = index as int;
                assert(left.spec_entries()[witness] == entry);
                assert(subtract as int == other.spec_quantity(entry.spec_kind()));
            }
            return EntrySubtraction::Underflow;
        }
        assert(left.0@.take(index as int + 1) == prefix.push(entry));
        proof { subtracted_entries_push(prefix, other, entry); }
        let remaining = entry.quantity().get() - subtract;
        if remaining != 0 {
            let next = ResourceEntry::new(entry.kind(), ResourceQuantity::from_wire(remaining));
            proof {
                assert(entries@.len() < u16::MAX as int);
                assert forall |prior: int| #![trigger entries@[prior]]
                    0 <= prior < entries@.len() implies
                    entries@[prior].spec_kind().spec_tag() < next.spec_kind().spec_tag() by {
                    assert(next.spec_kind() == entry.spec_kind());
                }
                ordered_push(entries@, next);
            }
            entries.push(next);
        }
        proof {
            if index + 1 < left.0.len() {
                assert(left.0@[index as int].spec_kind().spec_tag()
                    < left.0@[index as int + 1].spec_kind().spec_tag());
                assert forall |prior: int| #![trigger entries@[prior]]
                    0 <= prior < entries@.len() implies
                    entries@[prior].spec_kind().spec_tag()
                        < left.0@[index as int + 1].spec_kind().spec_tag() by {
                    if remaining != 0 && prior == entries@.len() - 1 {
                        assert(entries@[prior].spec_kind() == entry.spec_kind());
                    }
                }
            }
        }
        index += 1;
    }
    assert(left.0@.take(index as int) =~= left.0@);
    EntrySubtraction::Value(entries)
}

fn has_zero_sentinel(
    left: &ResourceVector,
    other: &ResourceVector,
) -> (result: bool)
    ensures result == subtraction_has_zero_sentinel(left, other),
{
    let mut other_index = 0usize;
    while other_index < other.0.len()
        invariant
            other_index <= other.0@.len(),
            forall |prior: int| 0 <= prior < other_index ==>
                left.spec_quantity(other.0@[prior].spec_kind()) != 0,
        decreases other.0@.len() - other_index,
    {
        if left.quantity(other.0[other_index].kind()) == 0 {
            assert(subtraction_has_zero_sentinel(left, other)) by {
                let witness = other_index as int;
                assert(other.spec_entries()[witness] == other.0[other_index as int]);
                assert(left.spec_quantity(other.0[other_index as int].spec_kind()) == 0);
            }
            return true;
        }
        other_index += 1;
    }
    assert(!subtraction_has_zero_sentinel(left, other)) by {
        assert forall |index: int| #![trigger other.spec_entries()[index]]
            0 <= index < other.spec_entries().len() implies
            left.spec_quantity(other.spec_entries()[index].spec_kind()) != 0 by {
        }
    }
    false
}

pub(super) fn subtract(
    left: &ResourceVector,
    other: &ResourceVector,
) -> (result: VectorSubtraction)
    ensures match result {
        VectorSubtraction::Underflow => subtraction_underflows(left, other),
        VectorSubtraction::AbsentDimension => {
            &&& !subtraction_underflows(left, other)
            &&& subtraction_has_zero_sentinel(left, other)
        }
        VectorSubtraction::ExactZero => {
            &&& !subtraction_underflows(left, other)
            &&& !subtraction_has_zero_sentinel(left, other)
            &&& subtracted_entries(left.spec_entries(), other).len() == 0
        }
        VectorSubtraction::Value(vector) => {
            &&& !subtraction_underflows(left, other)
            &&& !subtraction_has_zero_sentinel(left, other)
            &&& subtracted_entries(left.spec_entries(), other).len() > 0
            &&& subtraction_result(&vector, left, other)
        }
    },
{
    let entries = match subtract_entries(left, other) {
        EntrySubtraction::Underflow => return VectorSubtraction::Underflow,
        EntrySubtraction::Value(entries) => entries,
    };
    if has_zero_sentinel(left, other) {
        return VectorSubtraction::AbsentDimension;
    }

    if !entries.is_empty() {
        assert(crate::resource::capacity::entries_canonical(entries@));
        let vector = ResourceVector(entries);
        assert(subtraction_result(&vector, left, other));
        return VectorSubtraction::Value(vector);
    }
    assert(subtracted_entries(left.spec_entries(), other).len() == 0);
    VectorSubtraction::ExactZero
}

} // verus!

#[cfg(test)]
mod tests {
    use super::{ResourceEntry, ResourceQuantity, ResourceVector};
    use crate::{ResourceKind, SchedulerErrorKind};

    fn cpu_vector(quantity: u64) -> ResourceVector {
        match ResourceVector::new(
            vec![ResourceEntry::new(ResourceKind::CPU, ResourceQuantity::from_wire(quantity))],
            1,
        ) {
            Ok(vector) => vector,
            Err(error) => panic!("canonical unit vector was rejected: {error}"),
        }
    }

    #[test]
    fn zero_quantity_preserves_absent_dimension_and_underflow_precedence() {
        let zero = cpu_vector(0);
        let one = cpu_vector(1);

        let Err(absent) = zero.checked_subtract(&zero) else {
            panic!("zero sentinel was not rejected as an absent dimension");
        };
        assert_eq!(absent.kind(), SchedulerErrorKind::ResourceConflict);
        assert_eq!(absent.detail(), "resource subtraction names an absent dimension");

        let Err(underflow) = zero.checked_subtract(&one) else {
            panic!("underflow was not rejected before sentinel validation");
        };
        assert_eq!(underflow.kind(), SchedulerErrorKind::ResourceConflict);
        assert_eq!(underflow.detail(), "resource subtraction underflowed");
    }
}
