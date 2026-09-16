//! Exact identity equality and strict canonical byte-order laws.

use super::{DispatchId, WorkId, WorkerId};
#[cfg(verus_only)]
use core::cmp::Ordering;
use vstd::prelude::*;

verus! {

impl WorkId {
    /// Compares exact canonical work identities for verified lookup.
    pub(crate) const fn same(&self, other: &Self) -> (result: bool)
        ensures result == (*self == *other),
    {
        let mut index = 0;
        while index < 16
            invariant
                index <= 16,
                forall |prior: int| 0 <= prior < index ==>
                    self.0[prior] == other.0[prior],
            decreases 16 - index,
        {
            if self.0[index] != other.0[index] {
                assert(*self != *other);
                return false;
            }
            index += 1;
        }
        assert(self.0@ =~= other.0@) by {
            assert forall |at: int| 0 <= at < self.0@.len()
                implies self.0@[at] == other.0@[at] by {
            }
        }
        assert(self.0 == other.0);
        assert(*self == *other);
        true
    }

    /// Strict work-identity order never relates an identity to itself.
    pub(crate) proof fn order_irreflexive(value: &Self)
        ensures !value.spec_precedes(value),
    {
        order_reflexive_from(value.spec_bytes()@, 0);
    }

    /// Strict work-identity order cannot hold in both directions.
    pub(crate) proof fn order_asymmetric(left: &Self, right: &Self)
        requires left.spec_precedes(right),
        ensures !right.spec_precedes(left),
    {
        order_asymmetric_from(left.spec_bytes()@, right.spec_bytes()@, 0);
    }

    /// Strict work-identity order composes across an intermediate identity.
    pub(crate) proof fn order_transitive(left: &Self, middle: &Self, right: &Self)
        requires left.spec_precedes(middle), middle.spec_precedes(right),
        ensures left.spec_precedes(right),
    {
        order_transitive_from(
            left.spec_bytes()@,
            middle.spec_bytes()@,
            right.spec_bytes()@,
            0,
        );
    }

    /// Every pair of work identities is equal or strictly ordered.
    pub(crate) proof fn order_total(left: &Self, right: &Self)
        ensures *left == *right || left.spec_precedes(right) || right.spec_precedes(left),
    {
        order_total_from(left.spec_bytes()@, right.spec_bytes()@, 0);
        if peritus_types::canonical_byte_order_from(
            left.spec_bytes()@,
            right.spec_bytes()@,
            0,
        ) == Ordering::Equal {
            order_equal_from(left.spec_bytes()@, right.spec_bytes()@, 0);
            assert(left.spec_bytes()@ =~= right.spec_bytes()@);
            assert(left.spec_bytes() == right.spec_bytes());
            assert(*left == *right);
        }
    }
}

impl WorkerId {
    /// Compares exact canonical worker identities for verified lookup.
    pub(crate) const fn same(&self, other: &Self) -> (result: bool)
        ensures result == (*self == *other),
    {
        let mut index = 0;
        while index < 16
            invariant
                index <= 16,
                forall |prior: int| 0 <= prior < index ==>
                    self.0[prior] == other.0[prior],
            decreases 16 - index,
        {
            if self.0[index] != other.0[index] {
                assert(*self != *other);
                return false;
            }
            index += 1;
        }
        assert(self.0@ =~= other.0@) by {
            assert forall |at: int| 0 <= at < self.0@.len()
                implies self.0@[at] == other.0@[at] by {
            }
        }
        assert(self.0 == other.0);
        assert(*self == *other);
        true
    }

    /// Strict worker-identity order never relates an identity to itself.
    pub(crate) proof fn order_irreflexive(value: &Self)
        ensures !value.spec_precedes(value),
    {
        order_reflexive_from(value.spec_bytes()@, 0);
    }

    /// Strict worker-identity order cannot hold in both directions.
    pub(crate) proof fn order_asymmetric(left: &Self, right: &Self)
        requires left.spec_precedes(right),
        ensures !right.spec_precedes(left),
    {
        order_asymmetric_from(left.spec_bytes()@, right.spec_bytes()@, 0);
    }

    /// Strict worker-identity order composes across an intermediate identity.
    pub(crate) proof fn order_transitive(left: &Self, middle: &Self, right: &Self)
        requires left.spec_precedes(middle), middle.spec_precedes(right),
        ensures left.spec_precedes(right),
    {
        order_transitive_from(
            left.spec_bytes()@,
            middle.spec_bytes()@,
            right.spec_bytes()@,
            0,
        );
    }

    /// Every pair of worker identities is equal or strictly ordered.
    pub(crate) proof fn order_total(left: &Self, right: &Self)
        ensures *left == *right || left.spec_precedes(right) || right.spec_precedes(left),
    {
        order_total_from(left.spec_bytes()@, right.spec_bytes()@, 0);
        if peritus_types::canonical_byte_order_from(
            left.spec_bytes()@,
            right.spec_bytes()@,
            0,
        ) == Ordering::Equal {
            order_equal_from(left.spec_bytes()@, right.spec_bytes()@, 0);
            assert(left.spec_bytes()@ =~= right.spec_bytes()@);
            assert(left.spec_bytes() == right.spec_bytes());
            assert(*left == *right);
        }
    }
}

impl DispatchId {
    /// Compares exact dispatch identities for verified history membership.
    pub(crate) const fn same(&self, other: &Self) -> (result: bool)
        ensures result == (*self == *other),
    {
        let mut index = 0;
        while index < 16
            invariant
                index <= 16,
                forall |prior: int| 0 <= prior < index ==>
                    self.0[prior] == other.0[prior],
            decreases 16 - index,
        {
            if self.0[index] != other.0[index] {
                assert(*self != *other);
                return false;
            }
            index += 1;
        }
        assert(self.0@ =~= other.0@) by {
            assert forall |at: int| 0 <= at < self.0@.len()
                implies self.0@[at] == other.0@[at] by {
            }
        }
        assert(self.0 == other.0);
        assert(*self == *other);
        true
    }

    /// Strict dispatch-identity order never relates an identity to itself.
    pub(crate) proof fn order_irreflexive(value: &Self)
        ensures !value.spec_precedes(value),
    {
        order_reflexive_from(value.spec_bytes()@, 0);
    }

    /// Strict dispatch-identity order cannot hold in both directions.
    pub(crate) proof fn order_asymmetric(left: &Self, right: &Self)
        requires left.spec_precedes(right),
        ensures !right.spec_precedes(left),
    {
        order_asymmetric_from(left.spec_bytes()@, right.spec_bytes()@, 0);
    }

    /// Strict dispatch-identity order composes across an intermediate identity.
    pub(crate) proof fn order_transitive(left: &Self, middle: &Self, right: &Self)
        requires left.spec_precedes(middle), middle.spec_precedes(right),
        ensures left.spec_precedes(right),
    {
        order_transitive_from(
            left.spec_bytes()@,
            middle.spec_bytes()@,
            right.spec_bytes()@,
            0,
        );
    }

    /// Every pair of dispatch identities is equal or strictly ordered.
    pub(crate) proof fn order_total(left: &Self, right: &Self)
        ensures *left == *right || left.spec_precedes(right) || right.spec_precedes(left),
    {
        order_total_from(left.spec_bytes()@, right.spec_bytes()@, 0);
        if peritus_types::canonical_byte_order_from(
            left.spec_bytes()@,
            right.spec_bytes()@,
            0,
        ) == Ordering::Equal {
            order_equal_from(left.spec_bytes()@, right.spec_bytes()@, 0);
            assert(left.spec_bytes()@ =~= right.spec_bytes()@);
            assert(left.spec_bytes() == right.spec_bytes());
            assert(*left == *right);
        }
    }
}

proof fn order_reflexive_from(bytes: Seq<u8>, index: nat)
    requires index <= bytes.len(),
    ensures peritus_types::canonical_byte_order_from(bytes, bytes, index) == Ordering::Equal,
    decreases bytes.len() - index,
{
    if index < bytes.len() {
        order_reflexive_from(bytes, index + 1);
    }
}

proof fn order_asymmetric_from(left: Seq<u8>, right: Seq<u8>, index: nat)
    requires
        left.len() == right.len(),
        index <= left.len(),
        peritus_types::canonical_byte_order_from(left, right, index) == Ordering::Less,
    ensures peritus_types::canonical_byte_order_from(right, left, index) != Ordering::Less,
    decreases left.len() - index,
{
    if index < left.len() && left[index as int] == right[index as int] {
        order_asymmetric_from(left, right, index + 1);
    }
}

proof fn order_transitive_from(
    left: Seq<u8>,
    middle: Seq<u8>,
    right: Seq<u8>,
    index: nat,
)
    requires
        left.len() == middle.len(),
        middle.len() == right.len(),
        index <= left.len(),
        peritus_types::canonical_byte_order_from(left, middle, index) == Ordering::Less,
        peritus_types::canonical_byte_order_from(middle, right, index) == Ordering::Less,
    ensures peritus_types::canonical_byte_order_from(left, right, index) == Ordering::Less,
    decreases left.len() - index,
{
    if index < left.len()
        && left[index as int] == middle[index as int]
        && middle[index as int] == right[index as int]
    {
        order_transitive_from(left, middle, right, index + 1);
    }
}

proof fn order_total_from(left: Seq<u8>, right: Seq<u8>, index: nat)
    requires left.len() == right.len(), index <= left.len(),
    ensures
        peritus_types::canonical_byte_order_from(left, right, index) == Ordering::Equal
            || peritus_types::canonical_byte_order_from(left, right, index) == Ordering::Less
            || peritus_types::canonical_byte_order_from(right, left, index) == Ordering::Less,
    decreases left.len() - index,
{
    if index < left.len() && left[index as int] == right[index as int] {
        order_total_from(left, right, index + 1);
    }
}

proof fn order_equal_from(left: Seq<u8>, right: Seq<u8>, index: nat)
    requires index <= left.len(), index <= right.len(),
    ensures
        (peritus_types::canonical_byte_order_from(left, right, index) == Ordering::Equal)
            == (left.len() == right.len()
                && forall |at: int| index <= at < left.len() ==>
                    #[trigger] left[at] == #[trigger] right[at]),
    decreases left.len() - index,
{
    if index < left.len()
        && index < right.len()
        && left[index as int] == right[index as int]
    {
        order_equal_from(left, right, index + 1);
    }
}

} // verus!
