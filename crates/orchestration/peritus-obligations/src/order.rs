//! Exact lexicographic ordering of canonical identity bytes.

use core::cmp::Ordering;
use vstd::prelude::*;

verus! {

/// Mathematical byte order used by all canonical evidence identities.
pub open spec fn byte_order(left: Seq<u8>, right: Seq<u8>) -> Ordering {
    peritus_types::canonical_byte_order_from(left, right, 0)
}

/// Every adjacent identity is strictly increasing.
pub open spec fn ordered(keys: Seq<Seq<u8>>) -> bool {
    forall |index: int| 1 <= index < keys.len() ==>
        byte_order(#[trigger] keys[index - 1], keys[index]) == Ordering::Less
}

/// No two positions carry the same complete identity bytes.
pub open spec fn unique(keys: Seq<Seq<u8>>) -> bool {
    forall |left: int, right: int| 0 <= left < right < keys.len() ==>
        #[trigger] keys[left] != #[trigger] keys[right]
}

fn compare_from(left: &[u8], right: &[u8], index: usize) -> (order: Ordering)
    requires index <= left.len(), index <= right.len(),
    ensures order == peritus_types::canonical_byte_order_from(left@, right@, index as nat),
    decreases left.len() - index,
{
    if index == left.len() {
        if index == right.len() { Ordering::Equal } else { Ordering::Less }
    } else if index == right.len() {
        Ordering::Greater
    } else if left[index] < right[index] {
        Ordering::Less
    } else if left[index] > right[index] {
        Ordering::Greater
    } else {
        compare_from(left, right, index + 1)
    }
}

/// Compares complete identities without depending on unrefined derived ordering methods.
pub fn compare(left: &[u8], right: &[u8]) -> (order: Ordering)
    ensures order == byte_order(left@, right@),
        (order == Ordering::Equal) == (left@ == right@),
{
    let result = compare_from(left, right, 0);
    proof {
        equal_from(left@, right@, 0);
        if result == Ordering::Equal { assert(left@ =~= right@); }
    }
    result
}

proof fn equal_from(left: Seq<u8>, right: Seq<u8>, index: nat)
    requires index <= left.len(), index <= right.len(),
    ensures (peritus_types::canonical_byte_order_from(left, right, index) == Ordering::Equal)
        == (left.len() == right.len() && forall |i: int| index <= i < left.len() ==>
            #[trigger] left[i] == #[trigger] right[i]),
    decreases left.len() - index,
{
    if index < left.len() && index < right.len() && left[index as int] == right[index as int] {
        equal_from(left, right, index + 1);
    }
}

proof fn reflexive_from(bytes: Seq<u8>, index: nat)
    requires index <= bytes.len(),
    ensures peritus_types::canonical_byte_order_from(bytes, bytes, index) == Ordering::Equal,
    decreases bytes.len() - index,
{
    if index < bytes.len() { reflexive_from(bytes, index + 1); }
}

/// Canonical byte order is equal on identical complete byte sequences.
pub(crate) proof fn byte_order_reflexive(bytes: Seq<u8>)
    ensures byte_order(bytes, bytes) == Ordering::Equal,
{
    reflexive_from(bytes, 0);
}

proof fn transitive_from(left: Seq<u8>, middle: Seq<u8>, right: Seq<u8>, index: nat)
    requires
        left.len() == middle.len(), middle.len() == right.len(), index <= left.len(),
        peritus_types::canonical_byte_order_from(left, middle, index) == Ordering::Less,
        peritus_types::canonical_byte_order_from(middle, right, index) == Ordering::Less,
    ensures peritus_types::canonical_byte_order_from(left, right, index) == Ordering::Less,
    decreases left.len() - index,
{
    if index < left.len()
        && left[index as int] == middle[index as int]
        && middle[index as int] == right[index as int]
    {
        transitive_from(left, middle, right, index + 1);
    }
}

/// Composes two strict canonical byte-order facts of the same fixed width.
pub(crate) proof fn less_transitive(
    left: Seq<u8>,
    middle: Seq<u8>,
    right: Seq<u8>,
)
    requires
        left.len() == middle.len(),
        middle.len() == right.len(),
        byte_order(left, middle) == Ordering::Less,
        byte_order(middle, right) == Ordering::Less,
    ensures byte_order(left, right) == Ordering::Less,
{
    transitive_from(left, middle, right, 0);
}

/// Strict ordering relates any two positions in the canonical identity sequence.
pub proof fn ordered_pair(keys: Seq<Seq<u8>>, width: nat, left: int, right: int)
    requires
        ordered(keys),
        forall |index: int| 0 <= index < keys.len() ==> #[trigger] keys[index].len() == width,
        0 <= left < right < keys.len(),
    ensures byte_order(keys[left], keys[right]) == Ordering::Less,
    decreases right - left,
{
    assert(byte_order(keys[right - 1], keys[right]) == Ordering::Less);
    if left + 1 < right {
        ordered_pair(keys, width, left, right - 1);
        transitive_from(keys[left], keys[right - 1], keys[right], 0);
    }
}

/// A key below the query eliminates its position and every earlier ordered position.
pub(crate) proof fn less_eliminates_through(
    keys: Seq<Seq<u8>>,
    query: Seq<u8>,
    width: nat,
    middle: int,
)
    requires
        ordered(keys),
        forall |index: int| 0 <= index < keys.len() ==>
            #[trigger] keys[index].len() == width,
        query.len() == width,
        0 <= middle < keys.len(),
        byte_order(keys[middle], query) == Ordering::Less,
    ensures forall |index: int| 0 <= index <= middle ==>
        #[trigger] keys[index] != query,
{
    assert forall |index: int| 0 <= index <= middle implies
        #[trigger] keys[index] != query by {
        if index < middle {
            ordered_pair(keys, width, index, middle);
            less_transitive(keys[index], keys[middle], query);
            if keys[index] == query {
                byte_order_reflexive(query);
            }
        } else {
            assert(index == middle);
            if keys[index] == query {
                byte_order_reflexive(query);
            }
        }
    }
}

/// A key above the query eliminates its position and every later ordered position.
pub(crate) proof fn greater_eliminates_from(
    keys: Seq<Seq<u8>>,
    query: Seq<u8>,
    width: nat,
    middle: int,
)
    requires
        ordered(keys),
        forall |index: int| 0 <= index < keys.len() ==>
            #[trigger] keys[index].len() == width,
        query.len() == width,
        0 <= middle < keys.len(),
        byte_order(keys[middle], query) == Ordering::Greater,
    ensures forall |index: int| middle <= index < keys.len() ==>
        #[trigger] keys[index] != query,
{
    assert forall |index: int| middle <= index < keys.len() implies
        #[trigger] keys[index] != query by {
        if middle < index {
            ordered_pair(keys, width, middle, index);
            if keys[index] == query {
                assert(byte_order(keys[middle], query) == Ordering::Less);
            }
        } else {
            assert(index == middle);
            if keys[index] == query {
                byte_order_reflexive(query);
            }
        }
    }
}

/// Strict canonical ordering proves global identity uniqueness, not just adjacent inequality.
pub proof fn ordered_implies_unique(keys: Seq<Seq<u8>>, width: nat)
    requires
        ordered(keys),
        forall |index: int| 0 <= index < keys.len() ==> #[trigger] keys[index].len() == width,
    ensures unique(keys),
{
    assert forall |left: int, right: int| 0 <= left < right < keys.len() implies
        #[trigger] keys[left] != #[trigger] keys[right] by {
        ordered_pair(keys, width, left, right);
        if keys[left] == keys[right] { reflexive_from(keys[left], 0); }
    }
}

} // verus!
