//! Verified semantic equality used at the untrusted durable-CAS boundary.

mod aggregate;
mod binding;
mod record;

pub(super) use aggregate::aggregates_equal;

pub(super) use record::records_equal;

use super::LeaseCasExpectation;

pub(super) fn bytes16_equal_from(left: [u8; 16], right: [u8; 16], index: usize) -> bool {
    if index == 16 {
        true
    } else if left[index] != right[index] {
        false
    } else {
        bytes16_equal_from(left, right, index + 1)
    }
}
pub(super) fn bytes16_equal(left: [u8; 16], right: [u8; 16]) -> bool {
    bytes16_equal_from(left, right, 0)
}
pub(super) fn bytes32_equal_from(left: [u8; 32], right: [u8; 32], index: usize) -> bool {
    if index == 32 {
        true
    } else if left[index] != right[index] {
        false
    } else {
        bytes32_equal_from(left, right, index + 1)
    }
}
pub(super) fn bytes32_equal(left: [u8; 32], right: [u8; 32]) -> bool {
    bytes32_equal_from(left, right, 0)
}
pub(super) const fn expectations_equal(
    left: LeaseCasExpectation,
    right: LeaseCasExpectation,
) -> bool {
    match (left, right) {
        (LeaseCasExpectation::Absent, LeaseCasExpectation::Absent) => true,
        (
            LeaseCasExpectation::Version(left_version),
            LeaseCasExpectation::Version(right_version),
        ) => left_version.get() == right_version.get(),
        _ => false,
    }
}
