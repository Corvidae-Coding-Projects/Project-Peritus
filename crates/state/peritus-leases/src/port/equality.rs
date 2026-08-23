//! Verified semantic equality used at the untrusted durable-CAS boundary.

mod aggregate;
mod binding;
mod record;

pub(super) use aggregate::aggregates_equal;
#[cfg(verus_only)]
pub(super) use aggregate::aggregate_fields_match;
pub(super) use record::records_equal;
#[cfg(verus_only)]
pub(super) use record::record_fields_match;

use vstd::prelude::*;
use super::LeaseCasExpectation;

verus! {

pub(crate) open spec fn bytes16_match_from(
    left: [u8; 16],
    right: [u8; 16],
    index: nat,
) -> bool
    decreases 16 - index,
{
    if index >= 16 {
        true
    } else {
        left[index as int] == right[index as int]
            && bytes16_match_from(left, right, index + 1)
    }
}

pub(crate) open spec fn bytes16_match(left: [u8; 16], right: [u8; 16]) -> bool {
    bytes16_match_from(left, right, 0)
}

pub(super) fn bytes16_equal_from(
    left: [u8; 16],
    right: [u8; 16],
    index: usize,
) -> (equal: bool)
    requires index <= 16,
    ensures equal == bytes16_match_from(left, right, index as nat),
    decreases 16 - index,
{
    if index == 16 {
        true
    } else if left[index] != right[index] {
        false
    } else {
        bytes16_equal_from(left, right, index + 1)
    }
}

pub(super) fn bytes16_equal(left: [u8; 16], right: [u8; 16]) -> (equal: bool)
    ensures equal == bytes16_match(left, right),
{
    bytes16_equal_from(left, right, 0)
}

pub(crate) open spec fn bytes32_match_from(
    left: [u8; 32],
    right: [u8; 32],
    index: nat,
) -> bool
    decreases 32 - index,
{
    if index >= 32 {
        true
    } else {
        left[index as int] == right[index as int]
            && bytes32_match_from(left, right, index + 1)
    }
}

pub(crate) open spec fn bytes32_match(left: [u8; 32], right: [u8; 32]) -> bool {
    bytes32_match_from(left, right, 0)
}

pub(super) fn bytes32_equal_from(
    left: [u8; 32],
    right: [u8; 32],
    index: usize,
) -> (equal: bool)
    requires index <= 32,
    ensures equal == bytes32_match_from(left, right, index as nat),
    decreases 32 - index,
{
    if index == 32 {
        true
    } else if left[index] != right[index] {
        false
    } else {
        bytes32_equal_from(left, right, index + 1)
    }
}

pub(super) fn bytes32_equal(left: [u8; 32], right: [u8; 32]) -> (equal: bool)
    ensures equal == bytes32_match(left, right),
{
    bytes32_equal_from(left, right, 0)
}

pub(crate) open spec fn expectation_fields_match(
    left: LeaseCasExpectation,
    right: LeaseCasExpectation,
) -> bool {
    match (left, right) {
        (LeaseCasExpectation::Absent, LeaseCasExpectation::Absent) => true,
        (LeaseCasExpectation::Version(left_version), LeaseCasExpectation::Version(right_version)) => {
            left_version.spec_value() == right_version.spec_value()
        }
        _ => false,
    }
}

pub(super) const fn expectations_equal(
    left: LeaseCasExpectation,
    right: LeaseCasExpectation,
) -> (equal: bool)
    ensures equal == expectation_fields_match(left, right),
{
    match (left, right) {
        (LeaseCasExpectation::Absent, LeaseCasExpectation::Absent) => true,
        (LeaseCasExpectation::Version(left_version), LeaseCasExpectation::Version(right_version)) => {
            left_version.get() == right_version.get()
        }
        _ => false,
    }
}

} // verus!
