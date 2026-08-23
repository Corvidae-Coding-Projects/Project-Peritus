//! Exact credential-registry projections used by reducer contracts.

#[cfg(verus_only)]
use super::ApproverCredential;
use vstd::prelude::*;

verus! {

pub(super) open spec fn credential_from(
    entries: Seq<ApproverCredential>,
    key_id: crate::ApprovalKeyId,
    index: nat,
) -> Option<ApproverCredential>
    decreases entries.len() - index,
{
    if index >= entries.len() {
        None
    } else if crate::state::exact::same_digest_from(
        entries[index as int].spec_key_id().spec_bytes(),
        key_id.spec_bytes(),
        0,
    ) {
        Some(entries[index as int])
    } else {
        credential_from(entries, key_id, index + 1)
    }
}

} // verus!
