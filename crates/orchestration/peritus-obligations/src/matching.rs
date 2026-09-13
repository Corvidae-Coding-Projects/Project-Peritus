//! Exact byte identity comparisons shared by verified obligation kernels.

use crate::{AlternativeBranchId, AlternativeGroupId};
use vstd::prelude::*;

verus! {

/// Compares every byte of two alternative-group identities.
pub fn group_ids_match(
    left: AlternativeGroupId,
    right: AlternativeGroupId,
) -> (same: bool)
    ensures same == (left.spec_digest().spec_bytes()@ == right.spec_digest().spec_bytes()@),
{
    matches!(
        crate::order::compare(left.digest().as_bytes(), right.digest().as_bytes()),
        core::cmp::Ordering::Equal
    )
}

/// Compares every byte of two alternative-branch identities.
pub fn branch_ids_match(
    left: AlternativeBranchId,
    right: AlternativeBranchId,
) -> (same: bool)
    ensures same == (left.spec_digest().spec_bytes()@ == right.spec_digest().spec_bytes()@),
{
    matches!(
        crate::order::compare(left.digest().as_bytes(), right.digest().as_bytes()),
        core::cmp::Ordering::Equal
    )
}

} // verus!
