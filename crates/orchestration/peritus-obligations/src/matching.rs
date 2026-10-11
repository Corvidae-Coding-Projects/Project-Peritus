//! Exact byte identity comparisons shared by verified obligation kernels.

use crate::{AlternativeBranchId, AlternativeGroupId};

#[doc = " Compares every byte of two alternative-group identities."]
pub fn group_ids_match(left: AlternativeGroupId, right: AlternativeGroupId) -> bool {
    matches!(
        crate::order::compare(left.digest().as_bytes(), right.digest().as_bytes()),
        core::cmp::Ordering::Equal
    )
}
#[doc = " Compares every byte of two alternative-branch identities."]
pub fn branch_ids_match(left: AlternativeBranchId, right: AlternativeBranchId) -> bool {
    matches!(
        crate::order::compare(left.digest().as_bytes(), right.digest().as_bytes()),
        core::cmp::Ordering::Equal
    )
}
