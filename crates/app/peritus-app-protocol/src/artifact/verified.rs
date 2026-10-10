//! Executable and mathematical chunk-conservation predicates.

#[doc = " Returns whether a chunk begins at the conserved offset and remains within declared size."]
#[must_use]
pub fn chunk_is_contiguous(conserved: u64, offset: u64, chunk_bytes: usize, declared: u64) -> bool {
    if chunk_bytes as u128 > u128::from(u64::MAX) {
        return false;
    }
    let length = chunk_bytes as u64;
    offset == conserved && conserved <= declared && length <= declared - conserved
}
#[doc = " Returns whether exact byte conservation permits completion."]
#[must_use]
pub const fn completion_is_conserved(conserved: u64, declared: u64) -> bool {
    conserved == declared
}
