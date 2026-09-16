//! Executable and mathematical chunk-conservation predicates.

use vstd::prelude::*;

verus! {

/// Returns whether a chunk begins at the conserved offset and remains within declared size.
#[must_use]
pub fn chunk_is_contiguous(conserved: u64, offset: u64, chunk_bytes: usize, declared: u64) -> (contiguous: bool)
    ensures contiguous == (
        offset == conserved && conserved as int + chunk_bytes as int <= declared as int
    ),
{
    if chunk_bytes as u128 > u128::from(u64::MAX) {
        return false;
    }
    let length = chunk_bytes as u64;
    offset == conserved && conserved <= declared && length <= declared - conserved
}

/// Returns whether exact byte conservation permits completion.
#[must_use]
pub const fn completion_is_conserved(conserved: u64, declared: u64) -> (complete: bool)
    ensures complete == (conserved == declared),
{
    conserved == declared
}

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
