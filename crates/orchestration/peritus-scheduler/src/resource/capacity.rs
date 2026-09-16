//! Exact executable resource quantity and capacity correspondence.

use vstd::prelude::*;

use super::{ResourceEntry, ResourceKind, ResourceVector, VectorAdmission};
use core::cmp::Ordering;

verus! {

/// Canonical nonempty resource entries in strictly ascending adjacent tag order.
pub open spec fn entries_canonical(entries: Seq<ResourceEntry>) -> bool {
    &&& 0 < entries.len() <= u16::MAX as int
    &&& forall |index: int| #![trigger entries[index]] 1 <= index < entries.len() ==>
        entries[index - 1].spec_kind().spec_tag() < entries[index].spec_kind().spec_tag()
}

/// Exact public constructor admission under the supplied dimension ceiling.
pub open spec fn entries_admissible(
    entries: Seq<ResourceEntry>,
    maximum_dimensions: u16,
) -> bool {
    &&& 0 < entries.len() <= maximum_dimensions as int
    &&& forall |index: int| #![trigger entries[index]] 1 <= index < entries.len() ==>
        entries[index - 1].spec_kind().spec_tag() < entries[index].spec_kind().spec_tag()
}

/// Checks the same adjacent canonical order used by resource-vector admission.
pub(super) fn order_is_canonical(entries: &[ResourceEntry]) -> (result: bool)
    requires entries@.len() > 0,
    ensures result == (forall |index: int| #![trigger entries@[index]]
        1 <= index < entries@.len() ==>
        entries@[index - 1].spec_kind().spec_tag()
            < entries@[index].spec_kind().spec_tag()),
{
    let mut index = 1;
    while index < entries.len()
        invariant
            1 <= index <= entries@.len(),
            forall |prior: int| #![trigger entries@[prior]] 1 <= prior < index ==>
                entries@[prior - 1].spec_kind().spec_tag()
                    < entries@[prior].spec_kind().spec_tag(),
        decreases entries@.len() - index,
    {
        if entries[index - 1].kind().tag() >= entries[index].kind().tag() {
            return false;
        }
        index += 1;
    }
    true
}

/// Performs the exact production admission decision before public error construction.
pub(super) fn admit_entries(
    entries: Vec<ResourceEntry>,
    maximum_dimensions: u16,
) -> (result: VectorAdmission)
    ensures match result {
        VectorAdmission::Accepted(vector) => {
            &&& entries_admissible(entries@, maximum_dimensions)
            &&& vector.spec_entries() == entries@
        }
        VectorAdmission::LimitExceeded => {
            entries@.len() == 0 || entries@.len() > maximum_dimensions as int
        }
        VectorAdmission::NonCanonical => {
            &&& 0 < entries@.len() <= maximum_dimensions as int
            &&& !entries_admissible(entries@, maximum_dimensions)
        }
    },
{
    if entries.is_empty() || entries.len() > usize::from(maximum_dimensions) {
        VectorAdmission::LimitExceeded
    } else if !order_is_canonical(entries.as_slice()) {
        VectorAdmission::NonCanonical
    } else {
        assert(entries_canonical(entries@));
        VectorAdmission::Accepted(ResourceVector(entries))
    }
}

/// Returns the first exact quantity for a resource kind, or zero when absent.
pub open spec fn entries_quantity(
    entries: Seq<ResourceEntry>,
    kind: ResourceKind,
) -> int
    decreases entries.len(),
{
    if entries.len() == 0 {
        0
    } else if entries.first().spec_kind() == kind {
        entries.first().spec_quantity().spec_value() as int
    } else {
        entries_quantity(entries.drop_first(), kind)
    }
}

impl ResourceVector {
    /// Returns the mathematical quantity observed by the executable lookup.
    pub closed spec fn spec_quantity(&self, kind: ResourceKind) -> int {
        entries_quantity(self.spec_entries(), kind)
    }

    /// Returns whether every named request dimension fits in the capacity vector.
    pub closed spec fn spec_fits_within(&self, capacity: &Self) -> bool {
        entries_fit(self.spec_entries(), capacity)
    }

    pub(crate) proof fn quantity_matches_entries(&self, kind: ResourceKind)
        ensures self.spec_quantity(kind) == entries_quantity(self.spec_entries(), kind),
    {
    }
}

/// Returns whether each entry in a request sequence fits in one capacity vector.
pub open spec fn entries_fit(entries: Seq<ResourceEntry>, capacity: &ResourceVector) -> bool
    decreases entries.len(),
{
    entries.len() == 0 || {
        let entry = entries.first();
        entry.spec_quantity().spec_value() <= capacity.spec_quantity(entry.spec_kind())
            && entries_fit(entries.drop_first(), capacity)
    }
}

pub(super) proof fn earlier_tag_is_less(
    entries: Seq<ResourceEntry>,
    left: int,
    right: int,
)
    requires
        entries_canonical(entries),
        0 <= left < right < entries.len(),
    ensures
        entries[left].spec_kind().spec_tag() < entries[right].spec_kind().spec_tag(),
    decreases right - left,
{
    if left + 1 < right {
        earlier_tag_is_less(entries, left, right - 1);
        assert(entries[right - 1].spec_kind().spec_tag()
            < entries[right].spec_kind().spec_tag());
    }
}

proof fn drop_first_is_canonical(entries: Seq<ResourceEntry>)
    requires entries_canonical(entries), entries.len() > 1,
    ensures entries_canonical(entries.drop_first()),
{
    assert forall |index: int| #![trigger entries.drop_first()[index]]
        1 <= index < entries.drop_first().len() implies
        entries.drop_first()[index - 1].spec_kind().spec_tag()
            < entries.drop_first()[index].spec_kind().spec_tag() by {
        assert(entries.drop_first()[index - 1] == entries[index]);
        assert(entries.drop_first()[index] == entries[index + 1]);
    }
}

pub(super) proof fn exact_quantity_at(
    entries: Seq<ResourceEntry>,
    index: int,
    kind: ResourceKind,
)
    requires
        entries_canonical(entries),
        0 <= index < entries.len(),
        entries[index].spec_kind() == kind,
    ensures
        entries_quantity(entries, kind)
            == entries[index].spec_quantity().spec_value() as int,
    decreases entries.len(),
{
    if index > 0 {
        earlier_tag_is_less(entries, 0, index);
        assert(entries[0].spec_kind() != kind);
        drop_first_is_canonical(entries);
        exact_quantity_at(entries.drop_first(), index - 1, kind);
    }
}

proof fn absent_quantity_is_zero(entries: Seq<ResourceEntry>, kind: ResourceKind)
    requires
        forall |index: int| #![trigger entries[index]] 0 <= index < entries.len() ==>
            entries[index].spec_kind() != kind,
    ensures entries_quantity(entries, kind) == 0,
    decreases entries.len(),
{
    if entries.len() > 0 {
        assert(entries.first().spec_kind() != kind);
        assert forall |index: int| #![trigger entries.drop_first()[index]]
            0 <= index < entries.drop_first().len() implies
            entries.drop_first()[index].spec_kind() != kind by {
            assert(entries.drop_first()[index] == entries[index + 1]);
        }
        absent_quantity_is_zero(entries.drop_first(), kind);
    }
}

fn indexed_quantity(
    entries: &[ResourceEntry],
    kind: ResourceKind,
) -> (result: u64)
    requires entries_canonical(entries@),
    ensures result as int == entries_quantity(entries@, kind),
{
    let mut low = 0usize;
    let mut high = entries.len();
    while low < high
        invariant
            low <= high <= entries@.len(),
            entries_canonical(entries@),
            forall |prior: int| #![trigger entries@[prior]] 0 <= prior < low ==>
                entries@[prior].spec_kind() != kind,
            forall |later: int| #![trigger entries@[later]] high <= later < entries@.len() ==>
                entries@[later].spec_kind() != kind,
        decreases high - low,
    {
        let middle = low + (high - low) / 2;
        assert(low <= middle < high);
        let observed = entries[middle].kind().tag();
        let target = kind.tag();
        match observed.cmp(&target) {
            Ordering::Equal => {
                assert(observed == target);
                assert(entries@[middle as int].spec_kind() == kind);
                proof {
                    exact_quantity_at(entries@, middle as int, kind);
                }
                return entries[middle].quantity().get();
            }
            Ordering::Less => {
                assert(observed < target);
                assert forall |prior: int| #![trigger entries@[prior]]
                    0 <= prior <= middle implies
                    entries@[prior].spec_kind() != kind by {
                    if prior >= low && prior < middle {
                        earlier_tag_is_less(entries@, prior, middle as int);
                    }
                }
                low = middle + 1;
            }
            Ordering::Greater => {
                assert(observed > target);
                assert forall |later: int| #![trigger entries@[later]]
                    middle <= later < entries@.len() implies
                    entries@[later].spec_kind() != kind by {
                    if later > middle {
                        earlier_tag_is_less(entries@, middle as int, later);
                    }
                }
                high = middle;
            }
        }
    }
    assert forall |index: int| #![trigger entries@[index]]
        0 <= index < entries@.len() implies
        entries@[index].spec_kind() != kind by {
        if index < low {
        } else {
            assert(index >= high);
        }
    }
    proof {
        absent_quantity_is_zero(entries@, kind);
    }
    0
}

fn fits_from(
    entries: &[ResourceEntry],
    index: usize,
    capacity: &ResourceVector,
) -> (result: bool)
    requires index <= entries@.len(),
    ensures
        result
            == entries_fit(
                entries@.subrange(index as int, entries@.len() as int),
                capacity,
            ),
    decreases entries@.len() - index,
{
    if index == entries.len() {
        proof {
            assert(entries@.subrange(index as int, entries@.len() as int).len() == 0);
        }
        true
    } else {
        let ghost suffix = entries@.subrange(index as int, entries@.len() as int);
        assert(suffix.len() > 0);
        assert(suffix.first() == entries@[index as int]);
        assert(suffix.drop_first()
            =~= entries@.subrange(index as int + 1, entries@.len() as int));
        let entry = entries[index];
        entry.quantity().get() <= capacity.quantity(entry.kind())
            && fits_from(entries, index + 1, capacity)
    }
}

impl ResourceVector {
    /// Returns a dimension's quantity, with absence representing exact zero usage.
    #[must_use]
    pub fn quantity(&self, kind: ResourceKind) -> (result: u64)
        ensures result as int == self.spec_quantity(kind),
    {
        proof { use_type_invariant(self); }
        indexed_quantity(&self.0, kind)
    }

    /// Returns whether every requested quantity is provided by `capacity`.
    #[must_use]
    pub fn fits_within(&self, capacity: &Self) -> (result: bool)
        ensures result == self.spec_fits_within(capacity),
    {
        let result = fits_from(&self.0, 0, capacity);
        proof {
            assert(self.0@.subrange(0, self.0@.len() as int) =~= self.0@);
        }
        result
    }
}

} // verus!
