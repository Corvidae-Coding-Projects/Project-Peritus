//! Shared exact value relations for public reducer contracts.

use peritus_types::ActionId;

const fn identifier_values_equal_from(left: [u8; 16], right: [u8; 16], index: usize) -> bool {
    if index == 16 {
        true
    } else if left[index] != right[index] {
        false
    } else {
        identifier_values_equal_from(left, right, index + 1)
    }
}
pub const fn identifier_bytes_equal(left: [u8; 16], right: [u8; 16]) -> bool {
    identifier_values_equal_from(left, right, 0)
}
const fn digest_values_equal_from(left: [u8; 32], right: [u8; 32], index: usize) -> bool {
    if index == 32 {
        true
    } else if left[index] != right[index] {
        false
    } else {
        digest_values_equal_from(left, right, index + 1)
    }
}
pub const fn digest_bytes_equal(left: [u8; 32], right: [u8; 32]) -> bool {
    digest_values_equal_from(left, right, 0)
}
pub(super) const fn action_id_values_equal(left: ActionId, right: ActionId) -> bool {
    identifier_values_equal_from(*left.as_bytes(), *right.as_bytes(), 0)
}
pub(super) const fn action_digest_values_equal(
    left: crate::ActionDigest,
    right: crate::ActionDigest,
) -> bool {
    digest_values_equal_from(*left.sha256().as_bytes(), *right.sha256().as_bytes(), 0)
}
