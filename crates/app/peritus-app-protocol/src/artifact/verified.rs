//! Executable and mathematical chunk-conservation predicates.

use vstd::prelude::*;

/// Returns whether a chunk begins at the conserved offset and remains within declared size.
#[must_use]
pub fn chunk_is_contiguous(conserved: u64, offset: u64, chunk_bytes: usize, declared: u64) -> bool {
    offset == conserved
        && u64::try_from(chunk_bytes)
            .ok()
            .and_then(|length| conserved.checked_add(length))
            .is_some_and(|after| after <= declared)
}

/// Returns whether exact byte conservation permits completion.
#[must_use]
pub const fn completion_is_conserved(conserved: u64, declared: u64) -> bool {
    conserved == declared
}

verus! {

/// Mathematical chunk step for `INV-025 ChunkConservation`.
pub open spec fn spec_chunk_conserved(
    before: int,
    offset: int,
    chunk_length: int,
    declared: int,
    after: int,
) -> bool {
    0 <= before && offset == before && 0 < chunk_length
        && after == before + chunk_length && after <= declared
}

/// One legal chunk advances the conserved offset by exactly its length.
pub proof fn legal_chunk_conserves_bytes(
    before: int,
    offset: int,
    chunk_length: int,
    declared: int,
    after: int,
)
    requires
        spec_chunk_conserved(before, offset, chunk_length, declared, after),
    ensures
        after - before == chunk_length,
        after <= declared,
{
}

/// Exact conservation is necessary for legal completion.
pub proof fn completion_requires_declared_size(conserved: int, declared: int)
    requires
        conserved == declared,
    ensures
        conserved <= declared,
        declared <= conserved,
{
}

} // verus!
