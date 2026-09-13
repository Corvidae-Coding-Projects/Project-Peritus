//! Shared checked-construction predicates for release-policy values.

use crate::{ConstructionError, ConstructionErrorKind};
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

/// Logical predicate for a digest that is not the reserved all-zero placeholder.
pub open spec fn spec_digest_nonzero(digest: Sha256Digest) -> bool {
    exists |index: int| 0 <= index < 32 && digest.spec_bytes()[index] != 0
}

const fn digest_nonzero(digest: Sha256Digest) -> (nonzero: bool)
    ensures nonzero == spec_digest_nonzero(digest),
{
    digest_bytes_nonzero(digest.as_bytes())
}

/// Returns whether a fixed-width digest representation contains a nonzero byte.
pub const fn digest_bytes_nonzero(bytes: &[u8; 32]) -> (nonzero: bool)
    ensures nonzero == (exists |index: int| 0 <= index < 32 && (*bytes)[index] != 0),
{
    let mut index = 0;
    while index < bytes.len()
        invariant
            0 <= index <= bytes.len(),
            forall |prior: int| 0 <= prior < index ==> (*bytes)[prior] == 0,
        decreases bytes.len() - index,
    {
        if bytes[index] != 0 {
            assert(exists |witness: int| witness == index as int
                && 0 <= witness < 32 && (*bytes)[witness] != 0);
            return true;
        }
        index += 1;
    }
    false
}

/// Rejects the reserved all-zero digest placeholder.
pub const fn require_digest(
    digest: Sha256Digest,
) -> (result: Result<(), ConstructionError>)
    ensures
        result.is_ok() == spec_digest_nonzero(digest),
        match result {
            Err(error) => error.spec_kind() == ConstructionErrorKind::ZeroDigest,
            Ok(()) => true,
        },
{
    if digest_nonzero(digest) {
        Ok(())
    } else {
        Err(ConstructionError::new(ConstructionErrorKind::ZeroDigest))
    }
}

/// Rejects a zero revision used by an identity-bearing input.
pub const fn require_revision(
    revision: u64,
) -> (result: Result<(), ConstructionError>)
    ensures
        result.is_ok() == (revision > 0),
        match result {
            Ok(()) => true,
            Err(error) => error.spec_kind() == ConstructionErrorKind::ZeroRevision,
        },
{
    if revision == 0 {
        Err(ConstructionError::new(ConstructionErrorKind::ZeroRevision))
    } else {
        Ok(())
    }
}

} // verus!
